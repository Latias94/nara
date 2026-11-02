use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_asset::{AssetRecord, AssetRef, AssetReferenceResolver, StableAssetId};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
#[cfg(feature = "serde")]
use nara_reflect::ComponentDecodeContext;
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentRegistrySnapshot,
    ComponentValueMaterializationLimits, EntityReferenceTraversalLimits,
};

#[cfg(feature = "serde")]
use crate::draft::ExpandedSceneEntity;
use crate::draft::SceneDraft;
use crate::{
    CompiledSceneDocument, SceneDocument, SceneEntityId, SceneEntityProvenance, SceneEntityRecord,
    ScenePatchDocument, SourceDocumentBindingError, SourceDocumentKey,
    compiled::CompiledSceneAdmissionError,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_asset_ref,
        with_entity_reference_rewrite_error, with_public_locator, with_public_u64,
    },
    schema_authority::SchemaAuthority,
};

#[cfg(feature = "serde")]
use crate::ExpandedSceneDocument;
#[cfg(test)]
use crate::SourceDocumentKind;
mod declaration;
#[cfg(feature = "serde")]
mod native_preview;
mod overlay;
#[cfg(feature = "serde")]
pub(crate) use overlay::validate_claim_paths;

pub use declaration::{
    PrefabDeclarationSource, PrefabInstanceLocation, PrefabInstancePath, PrefabInstancePathSegment,
    PrefabOverlaySlot, PrefabRevert, SceneCompilationFailure, SceneCompilationFailureKind,
};
#[cfg(feature = "serde")]
pub(crate) use overlay::cost::{PrefabOverlayCost, PrefabOverlayCostError, PrefabOverlayCostKind};
pub(crate) use overlay::cost::{
    PrefabOverlayCostLimits, prefab_instance_cost, prefab_instance_cost_with_admission,
    scene_placements_cost,
};
pub use overlay::{
    PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, PrefabPropertyClaim,
    PrefabPropertyIntent,
};

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabDocument {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")
    )]
    pub inputs: std::collections::BTreeSet<nara_identity::ReferenceInputId>,
    pub placements: Vec<crate::SceneEntityPlacement>,
    pub entities: Vec<SceneEntityRecord>,
}

impl PrefabDocument {
    #[must_use]
    pub fn new(
        placements: impl IntoIterator<Item = crate::SceneEntityPlacement>,
        entities: impl IntoIterator<Item = SceneEntityRecord>,
    ) -> Self {
        let mut entities = entities.into_iter().collect::<Vec<_>>();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            inputs: Default::default(),
            placements: placements.into_iter().collect(),
            entities,
        }
    }

    /// Creates a flat prefab whose input record order is its root order.
    #[must_use]
    pub fn from_ordered_roots(entities: impl IntoIterator<Item = SceneEntityRecord>) -> Self {
        let entities = entities.into_iter().collect::<Vec<_>>();
        let placements = entities
            .iter()
            .map(|entity| crate::SceneEntityPlacement::root(entity.id.clone()))
            .collect::<Vec<_>>();
        Self::new(placements, entities)
    }

    pub fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }

    #[must_use]
    pub fn instantiate(&self) -> SceneDocument {
        let mut document = SceneDocument {
            placements: self.placements.clone(),
            entities: self.entities.clone(),
        };
        document.canonicalize();
        document
    }

    #[must_use]
    pub fn instantiate_with_patch(
        &self,
        registry: &ComponentRegistrySnapshot,
        patch: &ScenePatchDocument,
    ) -> PrefabInstantiationReport {
        let mut document = self.instantiate();
        let patch_report = patch.apply_to_scene(&mut document, registry);
        PrefabInstantiationReport::from_patch_report(document, patch_report)
    }

    #[must_use]
    pub fn instantiate_with_patch_and_asset_resolution(
        &self,
        registry: &ComponentRegistrySnapshot,
        patch: &ScenePatchDocument,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> PrefabInstantiationReport {
        let mut document = self.instantiate();
        let patch_report =
            patch.apply_to_scene_with_asset_resolution(&mut document, registry, asset_resolution);
        PrefabInstantiationReport::from_patch_report(document, patch_report)
    }

    #[must_use]
    pub fn validate(&self, registry: &ComponentRegistrySnapshot) -> DiagnosticReport {
        self.instantiate().validate(registry)
    }

    #[must_use]
    pub fn validate_with_asset_resolution(
        &self,
        registry: &ComponentRegistrySnapshot,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> DiagnosticReport {
        self.instantiate()
            .validate_with_asset_resolution(registry, asset_resolution)
    }

    #[must_use]
    pub fn compile<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
    ) -> SceneCompilationReport {
        self.compile_with_resolution(registry, resolver, None)
    }

    #[must_use]
    pub fn compile_with_asset_resolution<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> SceneCompilationReport {
        self.compile_with_resolution(registry, resolver, Some(asset_resolution))
    }

    fn compile_with_resolution<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
    ) -> SceneCompilationReport {
        let mut loader = |source: &AssetRef, _: &mut SceneCompilationWorkingSet| {
            Ok::<_, Infallible>(resolver.resolve_prefab(source))
        };
        let mut context = SceneCompilationContext::new(
            SchemaAuthority::Native(registry),
            &mut loader,
            SceneCompilationOptions::default(),
            asset_resolution,
        );
        context.compile_source_document(&self.placements, &self.entities, Some(&self.inputs))
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PrefabInstantiationReport {
    pub document: Option<SceneDocument>,
    pub inverse: Option<ScenePatchDocument>,
    pub diagnostics: DiagnosticReport,
}

impl PrefabInstantiationReport {
    fn from_patch_report(
        document: SceneDocument,
        patch_report: crate::ScenePatchReport,
    ) -> PrefabInstantiationReport {
        if patch_report.applied {
            return PrefabInstantiationReport {
                document: Some(document),
                inverse: patch_report.inverse,
                diagnostics: patch_report.diagnostics,
            };
        }

        PrefabInstantiationReport {
            document: None,
            inverse: None,
            diagnostics: patch_report.diagnostics,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabInstance {
    /// A primary native Prefab source selector. Auxiliary imported products are not sources.
    pub source: AssetRef,
    /// Chooses a forest mount or a canonical-checked source root that contributes to its anchor.
    pub binding: PrefabInstanceBinding,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub arguments: Option<PrefabReferenceArguments>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub overrides: Option<PrefabOverlay>,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PrefabInstance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PrefabInstanceWire {
            source: AssetRef,
            binding: PrefabInstanceBinding,
            #[serde(default)]
            arguments: Option<PrefabReferenceArguments>,
            #[serde(default)]
            overrides: Option<PrefabOverlay>,
        }

        let wire = PrefabInstanceWire::deserialize(deserializer)?;
        if !wire.source.is_primary() {
            return Err(serde::de::Error::custom(
                "Prefab source must select a primary product",
            ));
        }
        Ok(Self {
            source: wire.source,
            binding: wire.binding,
            arguments: wire.arguments,
            overrides: wire.overrides,
        })
    }
}

/// The explicit materialization relationship between an instance and its containing anchor.
///
/// A root binding never depends on the current root count. Its selected source and overlay must
/// yield exactly the named source entity as their only root, which then shares the anchor's
/// canonical identity. Declaration scopes and nested instance slots remain distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum PrefabInstanceBinding {
    Forest,
    Root {
        base: SourceDocumentKey,
        root: SceneEntityId,
    },
}

impl PrefabInstance {
    #[must_use]
    pub const fn forest(source: AssetRef) -> Self {
        Self {
            source,
            binding: PrefabInstanceBinding::Forest,
            arguments: None,
            overrides: None,
        }
    }

    #[must_use]
    pub const fn root(source: AssetRef, base: SourceDocumentKey, root: SceneEntityId) -> Self {
        Self {
            source,
            binding: PrefabInstanceBinding::Root { base, root },
            arguments: None,
            overrides: None,
        }
    }
}

/// Explicit bindings into a Prefab's declared input environment, evaluated in the containing
/// lexical occurrence. The source owner checks the actual selected canonical base.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabReferenceArguments {
    pub base: SourceDocumentKey,
    pub values: BTreeMap<nara_identity::ReferenceInputId, nara_identity::EntityReference>,
}

/// Resolves a lookup request to its admitted source identity and immutable document together.
///
/// Requests are lookup coordinates, not semantic identity. Aliases of the same source must return
/// the same key and document; distinct sources must not share a key. This applies equally to
/// code-authored and file-backed resolvers, including document-only expansion.
/// Auxiliary products must be refused before opening a native Prefab source. An explicit
/// primary product requires a matching admitted source-asset association. That association does
/// not change a location-authored document's canonical identity.
///
/// A bare document cannot satisfy the resolver contract:
///
/// ```compile_fail
/// use nara_asset::AssetRef;
/// use nara_scene::{PrefabDocument, PrefabSourceResolver};
/// struct BareDocument;
/// impl PrefabSourceResolver for BareDocument {
///     fn resolve_prefab(&self, _: &AssetRef) -> Option<&PrefabDocument> { None }
/// }
/// ```
pub trait PrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource>;
}

/// One resolver-owned immutable Prefab and its required canonical source identity.
#[derive(Debug, Clone)]
pub struct ResolvedPrefabSource {
    document: ResolvedPrefabDocument,
    key: SourceDocumentKey,
    source_asset: Option<StableAssetId>,
}

#[derive(Debug, Clone)]
enum ResolvedPrefabDocument {
    Raw(Arc<PrefabDocument>),
    #[cfg(feature = "serde")]
    Authoring(Arc<crate::PublishedAuthoringPrefabDocument>),
    #[cfg(feature = "serde")]
    AuthoringSource {
        document: Arc<PrefabDocument>,
        basis: crate::AuthoringSchemaBasis,
    },
}

impl ResolvedPrefabSource {
    /// Binds an immutable declaration allocation to its source owner's admitted identity.
    ///
    /// Keeping this value across another loading callback does not borrow the loader's cache.
    /// The Host still owns file/metadata receipts and activation-time revalidation.
    pub const fn bound(document: Arc<PrefabDocument>, key: SourceDocumentKey) -> Self {
        Self {
            document: ResolvedPrefabDocument::Raw(document),
            source_asset: Self::key_source_asset(&key),
            key,
        }
    }

    /// Retains one published source allocation, including its exact authoring schema basis.
    #[cfg(feature = "serde")]
    pub const fn bound_authoring(
        document: Arc<crate::PublishedAuthoringPrefabDocument>,
        key: SourceDocumentKey,
    ) -> Self {
        Self {
            document: ResolvedPrefabDocument::Authoring(document),
            source_asset: Self::key_source_asset(&key),
            key,
        }
    }

    /// Binds bounded raw composition data without claiming editable source publication.
    /// The source owner retains decoding/structure admission and actual IO receipts.
    #[cfg(feature = "serde")]
    pub const fn bound_authoring_source(
        document: Arc<PrefabDocument>,
        basis: crate::AuthoringSchemaBasis,
        key: SourceDocumentKey,
    ) -> Self {
        Self {
            document: ResolvedPrefabDocument::AuthoringSource { document, basis },
            source_asset: Self::key_source_asset(&key),
            key,
        }
    }

    const fn key_source_asset(key: &SourceDocumentKey) -> Option<StableAssetId> {
        match key.origin() {
            crate::SourceDocumentOrigin::StableAsset { stable_id } => Some(*stable_id),
            _ => None,
        }
    }

    /// Attaches the source owner's actually admitted metadata without changing document identity.
    ///
    /// The source owner still proves actual file/metadata admission and retains its receipts.
    /// Named sources have no intrinsic path claim; their owner establishes the association.
    pub fn with_source_asset(
        mut self,
        record: &AssetRecord,
    ) -> Result<Self, SourceDocumentBindingError> {
        if self.key.kind() != crate::SourceDocumentKind::Prefab {
            return Err(SourceDocumentBindingError::DocumentKind);
        }
        record.validate_source_document_key(&self.key)?;
        if self.source_asset.is_some_and(|id| id != record.stable_id()) {
            return Err(SourceDocumentBindingError::SourceKeyMismatch);
        }
        self.source_asset = Some(record.stable_id());
        Ok(self)
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        match &self.document {
            ResolvedPrefabDocument::Raw(document) => document,
            #[cfg(feature = "serde")]
            ResolvedPrefabDocument::Authoring(document) => document.document(),
            #[cfg(feature = "serde")]
            ResolvedPrefabDocument::AuthoringSource { document, .. } => document,
        }
    }

    /// Classification is retained only by a published authoring allocation. Raw composition
    /// sources share their Schema catalogue but own no record facts or preservation manifest.
    #[cfg(feature = "serde")]
    pub(crate) fn checked_classification_retained_bytes(&self) -> Option<usize> {
        match &self.document {
            ResolvedPrefabDocument::Authoring(document) => {
                document.checked_classification_retained_bytes()
            }
            ResolvedPrefabDocument::Raw(_) | ResolvedPrefabDocument::AuthoringSource { .. } => {
                Some(0)
            }
        }
    }

    pub(crate) fn matches_authority(&self, authority: SchemaAuthority<'_>) -> bool {
        match (&self.document, authority) {
            (
                ResolvedPrefabDocument::Raw(_),
                SchemaAuthority::Native(_) | SchemaAuthority::Mixed(_),
            ) => true,
            #[cfg(feature = "serde")]
            (ResolvedPrefabDocument::Authoring(document), SchemaAuthority::Authoring(basis)) => {
                document.schema_basis() == *basis
            }
            #[cfg(feature = "serde")]
            (
                ResolvedPrefabDocument::AuthoringSource { basis, .. },
                SchemaAuthority::Authoring(actual),
            ) => basis == actual,
            #[cfg(feature = "serde")]
            _ => false,
        }
    }

    #[must_use]
    pub const fn key(&self) -> &SourceDocumentKey {
        &self.key
    }

    pub(crate) fn matches_request(&self, request: &AssetRef) -> bool {
        request.is_primary()
            && request
                .as_product()
                .is_none_or(|reference| self.source_asset == Some(reference.source()))
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InMemoryPrefabSourceResolver {
    sources: BTreeMap<SourceDocumentKey, Arc<PrefabDocument>>,
    requests: Vec<(AssetRef, SourceDocumentKey)>,
}

impl InMemoryPrefabSourceResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_prefab(
        mut self,
        source: AssetRef,
        key: SourceDocumentKey,
        prefab: PrefabDocument,
    ) -> Self {
        self.insert(source, key, prefab);
        self
    }

    /// Publishes one canonical source document and associates its lookup request.
    ///
    /// Replacing a canonical key updates every alias of that source. The returned value is the
    /// previous document for that key, not the document previously addressed by the request.
    /// Rebinding a request releases its old source when no other request still addresses it.
    pub fn insert(
        &mut self,
        source: AssetRef,
        key: SourceDocumentKey,
        prefab: PrefabDocument,
    ) -> Option<PrefabDocument> {
        let previous_key = if let Some((_, existing_key)) = self
            .requests
            .iter_mut()
            .find(|(candidate, _)| candidate == &source)
        {
            Some(std::mem::replace(existing_key, key.clone()))
        } else {
            self.requests.push((source, key.clone()));
            None
        };
        let previous_key = previous_key.filter(|previous_key| previous_key != &key);
        let previous = self.sources.insert(key, Arc::new(prefab));
        if let Some(previous_key) = previous_key
            && !self.requests.iter().any(|(_, key)| *key == previous_key)
        {
            self.sources.remove(&previous_key);
        }
        previous.map(|document| Arc::unwrap_or_clone(document))
    }

    #[cfg(feature = "serde")]
    pub fn insert_file_candidate(
        &mut self,
        source: AssetRef,
        key: SourceDocumentKey,
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Option<PrefabDocument>, crate::SceneFilePublicationError> {
        Ok(self.insert(source, key, candidate.publish(registry)?.into_document()))
    }

    #[cfg(feature = "serde")]
    pub fn insert_file_candidate_with_asset_resolution(
        &mut self,
        source: AssetRef,
        key: SourceDocumentKey,
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistrySnapshot,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> Result<Option<PrefabDocument>, crate::SceneFilePublicationError> {
        let (prefab, _) = candidate.into_canonical_document(registry)?;
        let diagnostics = prefab
            .instantiate()
            .validate_authoring_with_asset_resolution(registry, asset_resolution);
        if diagnostics.has_errors() {
            return Err(crate::SceneFilePublicationError::new(diagnostics));
        }
        Ok(self.insert(source, key, prefab))
    }
}

impl PrefabSourceResolver for InMemoryPrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource> {
        if !source.is_primary() {
            return None;
        }
        self.requests
            .iter()
            .find(|(candidate, _)| candidate == source)
            .and_then(|(_, key)| {
                self.sources.get_key_value(key).map(|(key, prefab)| {
                    ResolvedPrefabSource::bound(Arc::clone(prefab), key.clone())
                })
            })
            .filter(|resolved| resolved.matches_request(source))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCompilationBudgetKind {
    MaterializedEntities,
    MaterializedComponents,
    MaterializedValueNodes,
    MaterializedValueBytes,
    MaterializedValueDepth,
    ResolvedInstances,
    OverlayDeclarations,
    GeneratedIdentifierBytes,
    SingleGeneratedIdentifierBytes,
    ProvenanceBytes,
    WorkingBytes,
    ReferenceWorkNodes,
    ReferenceWorkBytes,
}

impl SceneCompilationBudgetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MaterializedEntities => "materialized-entities",
            Self::MaterializedComponents => "materialized-components",
            Self::MaterializedValueNodes => "materialized-value-nodes",
            Self::MaterializedValueBytes => "materialized-value-bytes",
            Self::MaterializedValueDepth => "materialized-value-depth",
            Self::ResolvedInstances => "resolved-instances",
            Self::OverlayDeclarations => "overlay-declarations",
            Self::GeneratedIdentifierBytes => "generated-identifier-bytes",
            Self::SingleGeneratedIdentifierBytes => "single-generated-identifier-bytes",
            Self::ProvenanceBytes => "provenance-bytes",
            Self::WorkingBytes => "working-bytes",
            Self::ReferenceWorkNodes => "reference-work-nodes",
            Self::ReferenceWorkBytes => "reference-work-bytes",
        }
    }
}

impl Display for SceneCompilationBudgetKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneCompilationLimits {
    materialized_entities: ItemLimit,
    materialized_components: ItemLimit,
    materialized_value_nodes: ItemLimit,
    materialized_value_bytes: ByteLimit,
    materialized_value_depth: DepthLimit,
    source_value_limits: Option<ComponentValueMaterializationLimits>,
    resolved_instances: ItemLimit,
    overlay_declarations: ItemLimit,
    generated_identifier_bytes: ByteLimit,
    single_generated_identifier_bytes: ByteLimit,
    provenance_bytes: ByteLimit,
    working_bytes: ByteLimit,
    reference_work_nodes: ItemLimit,
    reference_work_bytes: ByteLimit,
}

impl Default for SceneCompilationLimits {
    fn default() -> Self {
        Self {
            materialized_entities: ItemLimit::new(100_000)
                .expect("prefab expansion entity limit is non-zero"),
            materialized_components: ItemLimit::new(500_000)
                .expect("prefab expansion component limit is non-zero"),
            materialized_value_nodes: ItemLimit::new(5_000_000)
                .expect("prefab expansion value node limit is non-zero"),
            materialized_value_bytes: ByteLimit::new(64 * 1024 * 1024)
                .expect("prefab expansion value byte limit is non-zero"),
            materialized_value_depth: EntityReferenceTraversalLimits::default().depth(),
            source_value_limits: None,
            resolved_instances: ItemLimit::new(100_000)
                .expect("prefab expansion instance limit is non-zero"),
            overlay_declarations: ItemLimit::new(100_000)
                .expect("prefab expansion overlay declaration limit is non-zero"),
            generated_identifier_bytes: ByteLimit::new(8 * 1024 * 1024)
                .expect("prefab expansion generated identifier byte limit is non-zero"),
            single_generated_identifier_bytes: ByteLimit::new(1024 * 1024)
                .expect("prefab expansion identifier byte limit is non-zero"),
            provenance_bytes: ByteLimit::new(64 * 1024 * 1024)
                .expect("prefab expansion provenance byte limit is non-zero"),
            working_bytes: ByteLimit::new(512 * 1024 * 1024)
                .expect("prefab expansion working byte limit is non-zero"),
            reference_work_nodes: ItemLimit::new(20_000_000)
                .expect("prefab reference work node limit is non-zero"),
            reference_work_bytes: ByteLimit::new(128 * 1024 * 1024)
                .expect("prefab reference work byte limit is non-zero"),
        }
    }
}

impl SceneCompilationLimits {
    /// Document-wide Schema, value and path work proving complete effective reference meaning.
    /// This includes defaults, shape and Native-leaf checks, unsuccessful/empty discovery and
    /// final local-reference closure. It is separate from stored value size and allocated bytes.
    #[must_use]
    pub const fn reference_work_nodes(self) -> ItemLimit {
        self.reference_work_nodes
    }

    #[must_use]
    pub const fn reference_work_bytes(self) -> ByteLimit {
        self.reference_work_bytes
    }

    #[must_use]
    pub const fn with_reference_work_nodes(mut self, limit: ItemLimit) -> Self {
        self.reference_work_nodes = limit;
        self
    }

    #[must_use]
    pub const fn with_reference_work_bytes(mut self, limit: ByteLimit) -> Self {
        self.reference_work_bytes = limit;
        self
    }

    /// Aggregate modeled allocation, independently enforced before owning allocations.
    #[must_use]
    pub const fn working_bytes(self) -> ByteLimit {
        self.working_bytes
    }

    #[must_use]
    pub const fn with_working_bytes(mut self, limit: ByteLimit) -> Self {
        self.working_bytes = limit;
        self
    }
    #[must_use]
    pub const fn materialized_entities(self) -> ItemLimit {
        self.materialized_entities
    }

    #[must_use]
    pub const fn materialized_components(self) -> ItemLimit {
        self.materialized_components
    }

    #[must_use]
    pub const fn materialized_value_nodes(self) -> ItemLimit {
        self.materialized_value_nodes
    }

    #[must_use]
    pub const fn materialized_value_bytes(self) -> ByteLimit {
        self.materialized_value_bytes
    }

    #[must_use]
    pub const fn resolved_instances(self) -> ItemLimit {
        self.resolved_instances
    }

    #[must_use]
    pub const fn overlay_declarations(self) -> ItemLimit {
        self.overlay_declarations
    }

    #[must_use]
    pub const fn generated_identifier_bytes(self) -> ByteLimit {
        self.generated_identifier_bytes
    }

    #[must_use]
    pub const fn single_generated_identifier_bytes(self) -> ByteLimit {
        self.single_generated_identifier_bytes
    }

    /// Cumulative modeled bytes admitted for source ancestry and entity provenance.
    ///
    /// Includes selected occurrence bindings and each owned copy of a projection chain, source key,
    /// request and entity identifier. Released entries do not refund this allocation budget.
    /// This is a logical ownership model, not an allocator or resident-memory limit.
    #[must_use]
    pub const fn provenance_bytes(self) -> ByteLimit {
        self.provenance_bytes
    }

    #[must_use]
    pub const fn with_materialized_entities(mut self, limit: ItemLimit) -> Self {
        self.materialized_entities = limit;
        self
    }

    #[must_use]
    pub const fn with_materialized_components(mut self, limit: ItemLimit) -> Self {
        self.materialized_components = limit;
        self
    }

    #[must_use]
    pub const fn with_materialized_value_nodes(mut self, limit: ItemLimit) -> Self {
        self.materialized_value_nodes = limit;
        self
    }

    #[must_use]
    pub const fn with_materialized_value_bytes(mut self, limit: ByteLimit) -> Self {
        self.materialized_value_bytes = limit;
        self
    }

    /// Complete value allowance used by diagnostic Native validation before migrations and codecs.
    pub(crate) const fn with_materialization_limits(
        mut self,
        limits: ComponentValueMaterializationLimits,
    ) -> Self {
        self.materialized_value_nodes = limits.nodes();
        self.materialized_value_bytes = limits.bytes();
        self.materialized_value_depth = limits.depth();
        self.source_value_limits = Some(limits);
        self
    }

    #[must_use]
    pub const fn with_resolved_instances(mut self, limit: ItemLimit) -> Self {
        self.resolved_instances = limit;
        self
    }

    #[must_use]
    pub const fn with_overlay_declarations(mut self, limit: ItemLimit) -> Self {
        self.overlay_declarations = limit;
        self
    }

    #[must_use]
    pub const fn with_generated_identifier_bytes(mut self, limit: ByteLimit) -> Self {
        self.generated_identifier_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_single_generated_identifier_bytes(mut self, limit: ByteLimit) -> Self {
        self.single_generated_identifier_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_provenance_bytes(mut self, limit: ByteLimit) -> Self {
        self.provenance_bytes = limit;
        self
    }

    fn materialization_limits(self) -> ComponentValueMaterializationLimits {
        ComponentValueMaterializationLimits::new(
            self.materialized_value_nodes(),
            self.materialized_value_bytes(),
            self.materialized_value_depth,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneCompilationOptions {
    pub max_depth: usize,
    pub limits: SceneCompilationLimits,
}

impl Default for SceneCompilationOptions {
    fn default() -> Self {
        Self {
            max_depth: 32,
            limits: SceneCompilationLimits::default(),
        }
    }
}

impl SceneCompilationOptions {
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    #[must_use]
    pub const fn with_limits(mut self, limits: SceneCompilationLimits) -> Self {
        self.limits = limits;
        self
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct SceneCompilationReport {
    /// The sealed flat document and its canonical provenance, absent after failed compilation.
    ///
    /// Its source owner must bind the containing source before materialization. Expansion does
    /// not infer that source or claim a runtime occurrence.
    pub candidate: Option<CompiledSceneDocument>,
    pub diagnostics: DiagnosticReport,
    /// The first rejected compiler step, independent of source Schema classification.
    pub failure: Option<SceneCompilationFailure>,
    source_upgrade_required: bool,
}

/// Disposable observations from compilation against one admitted data-authoring basis.
///
/// The candidate, omissions and terminal observations remain paired with the compiler's original
/// Schema evidence and working allowance until a consuming operation ends that custody.
/// Callers cannot replace the candidate or clear omissions before native preview restriction:
///
/// ```compile_fail,E0616
/// use nara_scene::__private::AuthoringSceneCompilationReport;
/// fn replace_candidate(report: &mut AuthoringSceneCompilationReport) {
///     report.candidate = None;
/// }
/// ```
///
/// ```compile_fail,E0616
/// use nara_scene::__private::AuthoringSceneCompilationReport;
/// fn clear_omissions(report: &mut AuthoringSceneCompilationReport) {
///     report.omissions.clear();
/// }
/// ```
#[cfg(feature = "serde")]
#[derive(Debug)]
pub struct AuthoringSceneCompilationReport {
    candidate: Option<ExpandedSceneDocument>,
    diagnostics: DiagnosticReport,
    failure: Option<SceneCompilationFailure>,
    omissions: Vec<AuthoringSceneCompilationOmission>,
    asset_references: Vec<nara_reflect::DeclaredAssetReference>,
    basis: crate::AuthoringSchemaBasis,
    usage: SceneCompilationUsage,
    limits: SceneCompilationLimits,
    reference_work: (usize, usize),
    budget_failed: bool,
}

#[cfg(feature = "serde")]
impl AuthoringSceneCompilationReport {
    /// Borrows the exact semantic candidate before native preview filtering.
    #[must_use]
    pub fn candidate(&self) -> Option<&ExpandedSceneDocument> {
        self.candidate.as_ref()
    }

    /// Borrows the original compiler diagnostics without changing its terminal outcome.
    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Borrows the first rejected compiler step, independently of source classification.
    #[must_use]
    pub fn failure(&self) -> Option<&SceneCompilationFailure> {
        self.failure.as_ref()
    }

    /// Borrows all compiler omissions paired with this candidate and working allowance.
    #[must_use]
    pub fn omissions(&self) -> &[AuthoringSceneCompilationOmission] {
        &self.omissions
    }

    /// Transfers the semantic observations without native preview restriction authority.
    ///
    /// The original Schema evidence and working allowance are not exposed or reconstructible
    /// from these parts. Choose this transfer or native preview conversion, not both:
    ///
    /// ```compile_fail,E0382
    /// use nara_reflect::ComponentRegistrySnapshot;
    /// use nara_scene::__private::AuthoringSceneCompilationReport;
    /// fn reuse_report(
    ///     report: AuthoringSceneCompilationReport,
    ///     registry: &ComponentRegistrySnapshot,
    /// ) {
    ///     let _parts = report.into_parts();
    ///     let _preview = report.into_native_preview_for(registry);
    /// }
    /// ```
    #[must_use]
    pub fn into_parts(self) -> AuthoringSceneCompilationParts {
        AuthoringSceneCompilationParts {
            candidate: self.candidate,
            diagnostics: self.diagnostics,
            failure: self.failure,
            omissions: self.omissions,
            effective_asset_references: self.asset_references,
        }
    }

    pub fn checked_work_bytes(&self) -> Option<usize> {
        self.usage.checked_work_bytes()
    }

    /// Tightens the original allowance before a consuming Native projection. Actual accumulated
    /// work remains charged; consumption refuses if that work already exceeds the new limit.
    pub fn restrict_working_bytes(&mut self, limit: ByteLimit) {
        if limit.get() < self.limits.working_bytes().get() {
            self.limits = self.limits.with_working_bytes(limit);
        }
    }

    pub const fn budget_exceeded(&self) -> bool {
        self.budget_failed
    }

    /// Actual effective values and defaults, before native preview component filtering.
    pub fn effective_asset_references(&self) -> &[nara_reflect::DeclaredAssetReference] {
        &self.asset_references
    }
}

/// Owned semantic observations after consuming the compiler report.
///
/// These parts carry no Schema evidence, compiler allowance or native preview conversion API.
/// Their candidate has not undergone presentation filtering or strict sealing. Native preview
/// requires consuming the original report while it still owns its basis and allowance.
#[cfg(feature = "serde")]
#[derive(Debug)]
pub struct AuthoringSceneCompilationParts {
    pub candidate: Option<ExpandedSceneDocument>,
    pub diagnostics: DiagnosticReport,
    pub failure: Option<SceneCompilationFailure>,
    pub omissions: Vec<AuthoringSceneCompilationOmission>,
    pub effective_asset_references: Vec<nara_reflect::DeclaredAssetReference>,
}

/// Presentation observations after consuming the original compiler's filtering allowance.
///
/// Diagnostics move with the filtered candidate; retaining them does not require a second copy.
/// This output carries no Schema basis, working allowance or further conversion authority.
#[cfg(feature = "serde")]
#[derive(Debug)]
pub struct AuthoringNativePreview {
    pub candidate: CompiledSceneDocument,
    pub omissions: Vec<crate::authoring_preview::AuthoringPreviewOmission>,
    pub diagnostics: DiagnosticReport,
    pub failure: Option<SceneCompilationFailure>,
    work_bytes: Option<usize>,
}

#[cfg(feature = "serde")]
impl AuthoringNativePreview {
    /// Complete accumulated compiler and sealing work, for the original Host reservation.
    pub const fn checked_work_bytes(&self) -> Option<usize> {
        self.work_bytes
    }
}

#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringSceneCompilationOmission {
    /// Projected locator in this report's immutable candidate; consult its typed provenance.
    pub entity: SceneEntityId,
    pub component: nara_reflect::ComponentTypeId,
    pub reason: AuthoringSceneCompilationOmissionReason,
}

#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringSceneCompilationOmissionReason {
    RecordUnavailable(crate::AuthoringRecordReadiness),
    NativeBindingUnavailable,
}

/// Compiles the same selected declaration program without native codecs or migrations.
/// Source publication, saving and history remain the containing source owner's responsibility.
#[cfg(feature = "serde")]
pub fn try_compile_authoring_scene_with_loader<F, E>(
    document: &SceneDocument,
    basis: &crate::AuthoringSchemaBasis,
    options: SceneCompilationOptions,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    mut loader: F,
) -> Result<AuthoringSceneCompilationReport, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut context = SceneCompilationContext::new(
        SchemaAuthority::Authoring(basis),
        &mut loader,
        options,
        asset_resolution,
    );
    let (candidate, failure) = context.compile_source_with_terminal(
        &document.placements,
        &document.entities,
        None,
        SceneCompilationContext::finish_authoring_document,
    );
    if let Some(error) = context.load_error {
        return Err(error);
    }
    Ok(AuthoringSceneCompilationReport {
        candidate,
        diagnostics: context.diagnostics,
        failure,
        omissions: context.omissions,
        asset_references: context.asset_references,
        basis: basis.clone(),
        usage: context.usage,
        limits: context.options.limits,
        reference_work: context.reference_work,
        budget_failed: context.budget_failed,
    })
}

/// An actual containing source, borrowed without a Scene-shaped surrogate.
#[cfg(feature = "serde")]
#[derive(Clone, Copy)]
pub(crate) enum SelectedInstanceSource<'a> {
    Scene(&'a SceneDocument),
    Prefab(&'a PrefabDocument),
}

/// A semantic fragment has no authority to stand in for a complete expanded document.
/// Unavailable surviving records are represented by the compilation's typed omissions.
#[cfg(feature = "serde")]
#[derive(Debug)]
pub(crate) struct SelectedInstanceFragment {
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) entities: Vec<ExpandedSceneEntity>,
    pub(crate) provenance: crate::SceneEntityProvenanceMap,
    pub(crate) targets: BTreeMap<PrefabEntityTarget, SceneEntityId>,
    pub(crate) reference_targets:
        BTreeMap<nara_identity::AuthoredEntityReference, nara_identity::EntityReference>,
}

#[cfg(feature = "serde")]
#[derive(Debug)]
pub(crate) struct SelectedInstanceCompilation {
    pub(crate) fragment: Option<SelectedInstanceFragment>,
    pub(crate) diagnostics: DiagnosticReport,
    pub(crate) failure: Option<SceneCompilationFailure>,
    pub(crate) omissions: Vec<AuthoringSceneCompilationOmission>,
    pub(crate) asset_references: Vec<nara_reflect::DeclaredAssetReference>,
    pub(crate) work_bytes: Option<usize>,
    pub(crate) budget_failed: bool,
}

/// Private comparison entry: only a directly owned anchor's complete projection is interpreted.
/// All containing-source IDs and topology remain actual external reference context.
#[cfg(feature = "serde")]
pub(crate) fn try_compile_selected_authoring_instance_with_loader<F, E>(
    source: SelectedInstanceSource<'_>,
    anchor: &SceneEntityId,
    reference_queries: &[nara_identity::AuthoredEntityReference],
    initial_reference_work: (usize, usize),
    basis: &crate::AuthoringSchemaBasis,
    options: SceneCompilationOptions,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    mut loader: F,
) -> Result<SelectedInstanceCompilation, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut context = SceneCompilationContext::new(
        SchemaAuthority::Authoring(basis),
        &mut loader,
        options,
        asset_resolution,
    );
    let result: Result<SelectedInstanceFragment, SceneCompilationFailure> = (|| {
        context
            .admit_terminal_work(initial_reference_work.0, initial_reference_work.1, Some(0))
            .map_err(|()| SceneCompilationFailureKind::WorkLimit)?;
        if !context.reserve_provenance_bytes(
            Some(std::mem::size_of::<crate::SceneEntityProvenanceMap>()),
            None,
            None,
        ) || !context.reserve_scratch_bytes(
            std::mem::size_of::<SelectedInstanceCompilation>()
                .checked_add(std::mem::size_of::<SelectedInstanceFragment>()),
            None,
            None,
        ) {
            return Err(SceneCompilationFailureKind::WorkLimit.into());
        }
        let output = overlay::compile::expand_selected_instance(
            &mut context,
            source,
            anchor,
            reference_queries,
        )?;
        let mut document = output.document;
        document.canonicalize();
        context.observe_authoring_omissions(&mut document);
        if !context.diagnostics.has_errors() {
            // The staged layout retains real containing-source shells. Their IDs close
            // references, but they have no component programs in selected demand.
            let validation = context.validate_authoring_document(&document);
            context.diagnostics.extend(validation);
        }
        if context.diagnostics.has_errors() {
            return Err(if context.budget_failed {
                SceneCompilationFailureKind::WorkLimit
            } else {
                SceneCompilationFailureKind::OwnerAdmission
            }
            .into());
        }
        // These source-only snapshots have their own lexical proof and allocation admission.
        // Restore them only after the flat effective values passed their ordinary validator.
        for defaults in output.source_defaults {
            let index = document
                .entities
                .binary_search_by(|record| record.id.cmp(&defaults.entity))
                .map_err(|_| SceneCompilationFailureKind::OwnerAdmission)?;
            let record = document.entities[index]
                .components
                .get_mut(&defaults.component)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            if !record.contextual_defaults.is_empty() {
                return Err(SceneCompilationFailureKind::OwnerAdmission.into());
            }
            record.contextual_defaults = defaults.values;
        }
        let root_bound = match source {
            SelectedInstanceSource::Scene(document) => &document.entities,
            SelectedInstanceSource::Prefab(document) => &document.entities,
        }
        .iter()
        .find(|record| &record.id == anchor)
        .and_then(|record| record.prefab.as_ref())
        .is_some_and(|instance| matches!(instance.binding, PrefabInstanceBinding::Root { .. }));
        context.provenance.retain(|id, origin| {
            (root_bound && id == anchor)
                || origin
                    .projection_path()
                    .is_some_and(|path| path.root_anchor() == anchor)
        });
        let record = document
            .entities
            .iter_mut()
            .find(|record| &record.id == anchor)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        let roots = if root_bound {
            vec![anchor.clone()]
        } else {
            record
                .children
                .iter()
                .filter(|id| context.provenance.contains_key(*id))
                .cloned()
                .collect()
        };
        document
            .entities
            .retain(|record| context.provenance.contains_key(&record.id));
        // Conversion also needs source routes for external projected reference endpoints.
        // This complete table belongs to the actual containing compilation, not to a
        // reconstruction from generated ID spelling or the selected fragment's provenance.
        let targets = output.targets;
        Ok(SelectedInstanceFragment {
            roots,
            entities: document.entities,
            provenance: std::mem::take(&mut context.provenance),
            targets,
            reference_targets: output.reference_targets,
        })
    })();
    let (fragment, failure) = match result {
        Ok(fragment) => (Some(fragment), None),
        Err(failure) => {
            context.push_expansion_failure(&failure);
            (None, Some(failure))
        }
    };
    let work_bytes = context.usage.checked_work_bytes();
    if let Some(error) = context.load_error {
        return Err(error);
    }
    Ok(SelectedInstanceCompilation {
        fragment,
        diagnostics: context.diagnostics,
        failure,
        omissions: context.omissions,
        asset_references: context.asset_references,
        work_bytes,
        budget_failed: context.budget_failed,
    })
}

impl SceneCompilationReport {
    /// Whether a retained source component was migrated during successful compilation.
    /// Discarded component payloads do not contribute to this observation.
    #[must_use]
    pub const fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    /// Borrows the sealed document before its source owner binds the containing source.
    #[must_use]
    pub fn document(&self) -> Option<&CompiledSceneDocument> {
        self.candidate.as_ref()
    }

    /// Borrows the source association established by this expansion.
    #[must_use]
    pub fn provenance(&self) -> Option<&crate::SceneEntityProvenanceMap> {
        self.candidate
            .as_ref()
            .map(CompiledSceneDocument::canonical_provenance)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct SceneCompilationUsage {
    materialized_entities: usize,
    materialized_components: usize,
    materialized_value_nodes: usize,
    materialized_value_bytes: usize,
    resolved_instances: usize,
    overlay_declarations: usize,
    generated_identifier_bytes: usize,
    provenance_bytes: usize,
    scratch_bytes: usize,
}

impl SceneCompilationUsage {
    fn checked_work_bytes(self) -> Option<usize> {
        expansion_work_bytes(
            [
                self.materialized_entities,
                self.materialized_components,
                self.materialized_value_nodes,
                self.resolved_instances,
                self.overlay_declarations,
            ],
            self.materialized_value_bytes,
            self.generated_identifier_bytes,
            self.provenance_bytes,
        )?
        .checked_add(self.scratch_bytes)
    }
}

/// One terminal ledger admits traversal work and allocation requests for both strict sealing and
/// authoring observations. An allocation overflow is distinct from an allocation-free callback.
fn admit_prefab_terminal_work(
    limits: SceneCompilationLimits,
    usage: &mut SceneCompilationUsage,
    reference_work: &mut (usize, usize),
    nodes: usize,
    bytes: usize,
    allocation: Option<usize>,
) -> Result<(), (SceneCompilationBudgetKind, usize, usize)> {
    reference_work.0 = reference_work.0.saturating_add(nodes);
    reference_work.1 = reference_work.1.saturating_add(bytes);
    for (kind, observed, maximum) in [
        (
            SceneCompilationBudgetKind::ReferenceWorkNodes,
            reference_work.0,
            limits.reference_work_nodes().get(),
        ),
        (
            SceneCompilationBudgetKind::ReferenceWorkBytes,
            reference_work.1,
            limits.reference_work_bytes().get(),
        ),
    ] {
        if observed > maximum {
            return Err((kind, observed, maximum));
        }
    }
    let mut next = *usage;
    let scratch = allocation.and_then(|allocation| next.scratch_bytes.checked_add(allocation));
    next.scratch_bytes = scratch.unwrap_or(usize::MAX);
    let work = scratch.and_then(|_| next.checked_work_bytes());
    let maximum = limits.working_bytes().get();
    if work.is_none_or(|work| work > maximum) {
        return Err((
            SceneCompilationBudgetKind::WorkingBytes,
            work.unwrap_or(usize::MAX),
            maximum,
        ));
    }
    *usage = next;
    Ok(())
}

/// Conservative modeled allocation held by a suspended expansion, excluding resolver caches.
/// Raw clones and derived scratch are cumulatively charged without refunds to private work.
#[derive(Debug, Clone, Copy)]
pub struct SceneCompilationWorkingSet {
    bytes: Option<usize>,
    working_limit: ByteLimit,
    source_depth: usize,
}

impl SceneCompilationWorkingSet {
    /// Actual selected source depth. The first requested Prefab has depth one.
    #[must_use]
    pub const fn source_depth(&self) -> usize {
        self.source_depth
    }
    #[must_use]
    pub const fn checked_work_bytes(&self) -> Option<usize> {
        self.bytes
    }

    /// Tightens the resumed compiler's budget after the loader retains additional work.
    /// A callback cannot enlarge the original allowance. The compiler rechecks the live
    /// model immediately on return, before cloning the newly resolved source.
    pub fn restrict_working_bytes(&mut self, limit: ByteLimit) {
        if limit.get() < self.working_limit.get() {
            self.working_limit = limit;
        }
    }

    #[cfg(feature = "serde")]
    pub(crate) const fn working_byte_limit(&self) -> ByteLimit {
        self.working_limit
    }

    /// A wrapper's loader sees its retained data as part of the suspended work.
    /// Restore only that wrapper's debit in this copy; the compiler packet stays unchanged.
    #[cfg(feature = "serde")]
    pub(crate) fn including_retained_working_bytes(self, retained: usize) -> Option<Self> {
        Some(Self {
            bytes: Some(self.bytes?.checked_add(retained)?),
            working_limit: ByteLimit::new(self.working_limit.get().checked_add(retained)?)?,
            ..self
        })
    }
}

// One Scene-owned model serves both the suspended snapshot and the Host's full reservation.
fn expansion_work_bytes(
    counts: [usize; 5],
    value_bytes: usize,
    identifiers: usize,
    provenance: usize,
) -> Option<usize> {
    counts
        .into_iter()
        .zip([128, 96, 96, 128, 256])
        .try_fold(0_usize, |bytes, (count, unit)| {
            bytes.checked_add(count.checked_mul(unit)?)
        })?
        .checked_add(value_bytes)?
        .checked_add(identifiers)?
        .checked_add(provenance)
}

impl SceneCompilationLimits {
    /// Upper reservation for the same allocation model reported to loading callbacks.
    #[must_use]
    pub fn checked_work_bytes(self) -> Option<usize> {
        // Scratch has no independent dimension ceiling. The enforced aggregate ceiling is
        // the reservation, including all raw clones and temporary component values.
        Some(self.working_bytes().get())
    }
}

impl SceneDocument {
    #[must_use]
    pub fn compile<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
    ) -> SceneCompilationReport {
        self.compile_with_options(registry, resolver, SceneCompilationOptions::default())
    }

    #[must_use]
    pub fn compile_with_asset_resolution<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> SceneCompilationReport {
        self.compile_with_options_and_asset_resolution(
            registry,
            resolver,
            SceneCompilationOptions::default(),
            asset_resolution,
        )
    }

    #[must_use]
    pub fn compile_with_options<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
        options: SceneCompilationOptions,
    ) -> SceneCompilationReport {
        match self.try_compile_with_loader(registry, options, None, |source, _| {
            Ok::<_, Infallible>(resolver.resolve_prefab(source))
        }) {
            Ok(report) => report,
            Err(error) => match error {},
        }
    }

    #[must_use]
    pub fn compile_with_options_and_asset_resolution<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistrySnapshot,
        resolver: &R,
        options: SceneCompilationOptions,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> SceneCompilationReport {
        match self.try_compile_with_loader(
            registry,
            options,
            Some(asset_resolution),
            |source, _| Ok::<_, Infallible>(resolver.resolve_prefab(source)),
        ) {
            Ok(report) => report,
            Err(error) => match error {},
        }
    }

    /// Compiles selected instance declarations while acquiring only their actual source demand.
    ///
    /// The loader returns shared immutable declarations with canonical identity and retains its
    /// own input budgets and receipts. It is called after enclosing instance selection. Its
    /// concrete error is returned unchanged; source/overlay conflicts remain in the report.
    /// The second callback argument includes all pre-admitted clones and compiler scratch
    /// held while loading is suspended. Resolver caches and decoding peaks belong to the loader.
    pub fn try_compile_with_loader<F, E>(
        &self,
        registry: &ComponentRegistrySnapshot,
        options: SceneCompilationOptions,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        loader: F,
    ) -> Result<SceneCompilationReport, E>
    where
        F: FnMut(
            &AssetRef,
            &mut SceneCompilationWorkingSet,
        ) -> Result<Option<ResolvedPrefabSource>, E>,
    {
        try_compile_with_authority(
            self,
            SchemaAuthority::Native(registry),
            options,
            asset_resolution,
            loader,
        )
    }
}

impl crate::SceneComponentBindings {
    /// Compiles native and explicitly admitted data records through the ordinary Prefab compiler.
    /// Data records require an exact durable owner claim and current Schema version. This does
    /// not install runtime components or establish gameplay callback availability.
    #[must_use]
    pub fn compile_scene<R: PrefabSourceResolver + ?Sized>(
        &self,
        document: &SceneDocument,
        resolver: &R,
    ) -> SceneCompilationReport {
        match self.try_compile_scene_with_loader(
            document,
            SceneCompilationOptions::default(),
            None,
            |source, _| Ok::<_, Infallible>(resolver.resolve_prefab(source)),
        ) {
            Ok(report) => report,
            Err(error) => match error {},
        }
    }

    /// Acquires only effective source demand under this immutable representation selection.
    /// Loader errors and working-set restrictions follow
    /// [`SceneDocument::try_compile_with_loader`]; no native-preview omission is used.
    pub fn try_compile_scene_with_loader<F, E>(
        &self,
        document: &SceneDocument,
        options: SceneCompilationOptions,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        loader: F,
    ) -> Result<SceneCompilationReport, E>
    where
        F: FnMut(
            &AssetRef,
            &mut SceneCompilationWorkingSet,
        ) -> Result<Option<ResolvedPrefabSource>, E>,
    {
        try_compile_with_authority(
            document,
            SchemaAuthority::Mixed(self),
            options,
            asset_resolution,
            loader,
        )
    }
}

fn try_compile_with_authority<F, E>(
    document: &SceneDocument,
    authority: SchemaAuthority<'_>,
    options: SceneCompilationOptions,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    mut loader: F,
) -> Result<SceneCompilationReport, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut context =
        SceneCompilationContext::new(authority, &mut loader, options, asset_resolution);
    let report = context.compile_scene_document(document);
    match context.load_error {
        Some(error) => Err(error),
        None => Ok(report),
    }
}

struct SceneCompilationContext<'a, F, E> {
    authority: SchemaAuthority<'a>,
    resolver: &'a mut F,
    load_error: Option<E>,
    options: SceneCompilationOptions,
    asset_resolution: Option<AssetReferenceResolver<'a>>,
    usage: SceneCompilationUsage,
    reference_work: (usize, usize),
    budget_failed: bool,
    source_upgrade_required: bool,
    diagnostics: DiagnosticReport,
    provenance: BTreeMap<SceneEntityId, SceneEntityProvenance>,
    reserved_entity_ids: BTreeSet<SceneEntityId>,
    next_projected_entity: usize,
    #[cfg(feature = "serde")]
    asset_references: Vec<nara_reflect::DeclaredAssetReference>,
    #[cfg(feature = "serde")]
    omissions: Vec<AuthoringSceneCompilationOmission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefabReferenceRemapError {
    SingleGeneratedIdentifier { observed: usize, maximum: usize },
    GeneratedIdentifier { observed: usize, maximum: usize },
    MaterializedValue { observed: usize, maximum: usize },
    MissingSourceEntity,
}

impl<'a, F, E> SceneCompilationContext<'a, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    fn new(
        authority: SchemaAuthority<'a>,
        resolver: &'a mut F,
        options: SceneCompilationOptions,
        asset_resolution: Option<AssetReferenceResolver<'a>>,
    ) -> Self {
        Self {
            authority,
            resolver,
            load_error: None,
            options,
            asset_resolution,
            usage: SceneCompilationUsage::default(),
            reference_work: (0, 0),
            budget_failed: false,
            source_upgrade_required: false,
            diagnostics: DiagnosticReport::default(),
            provenance: BTreeMap::new(),
            reserved_entity_ids: BTreeSet::new(),
            next_projected_entity: 0,
            #[cfg(feature = "serde")]
            asset_references: Vec::new(),
            #[cfg(feature = "serde")]
            omissions: Vec::new(),
        }
    }

    fn compile_scene_document(&mut self, document: &SceneDocument) -> SceneCompilationReport {
        self.compile_source_document(&document.placements, &document.entities, None)
    }

    fn compile_source_document(
        &mut self,
        placements: &[crate::SceneEntityPlacement],
        entities: &[SceneEntityRecord],
        inputs: Option<&BTreeSet<nara_identity::ReferenceInputId>>,
    ) -> SceneCompilationReport {
        let (candidate, failure) =
            self.compile_source_with_terminal(placements, entities, inputs, Self::seal_document);
        SceneCompilationReport {
            source_upgrade_required: candidate.is_some() && self.source_upgrade_required,
            candidate,
            diagnostics: std::mem::take(&mut self.diagnostics),
            failure,
        }
    }

    fn compile_source_with_terminal<T>(
        &mut self,
        placements: &[crate::SceneEntityPlacement],
        entities: &[SceneEntityRecord],
        inputs: Option<&BTreeSet<nara_identity::ReferenceInputId>>,
        terminal: impl FnOnce(&mut Self, SceneDraft) -> Option<T>,
    ) -> (Option<T>, Option<SceneCompilationFailure>) {
        if !self.reserve_provenance_bytes(
            Some(std::mem::size_of::<crate::SceneEntityProvenanceMap>()),
            None,
            None,
        ) {
            return (None, Some(SceneCompilationFailureKind::WorkLimit.into()));
        }
        let mut expanded = match overlay::compile::expand_source(self, placements, entities, inputs)
        {
            Ok(document) => document,
            Err(failure) => {
                self.push_expansion_failure(&failure);
                return (None, Some(failure));
            }
        };
        expanded.canonicalize();

        #[cfg(feature = "serde")]
        if !self.diagnostics.has_errors() {
            self.observe_authoring_omissions(&mut expanded);
        }

        if self.diagnostics.has_errors() {
            return (None, None);
        }
        (terminal(self, expanded), None)
    }

    fn seal_document(&mut self, document: SceneDraft) -> Option<CompiledSceneDocument> {
        let authority = self.authority;
        let limits = self.materialization_limits();
        let asset_resolution = self.asset_resolution;
        let provenance = std::mem::take(&mut self.provenance);
        match crate::compiled::admit_draft(
            document,
            provenance,
            authority,
            limits,
            asset_resolution,
            |nodes, bytes, allocation| self.admit_terminal_work(nodes, bytes, allocation),
        ) {
            Ok(document) => Some(document),
            Err(CompiledSceneAdmissionError::Admission(())) => None,
            Err(CompiledSceneAdmissionError::Diagnostics(diagnostics)) => {
                self.diagnostics.extend(diagnostics);
                None
            }
        }
    }

    #[cfg(feature = "serde")]
    fn finish_authoring_document(&mut self, document: SceneDraft) -> Option<ExpandedSceneDocument> {
        let validation = self.validate_authoring_document(&document);
        self.diagnostics.extend(validation);
        if self.diagnostics.has_errors() {
            return None;
        }
        match ExpandedSceneDocument::from_compilation(
            document,
            std::mem::take(&mut self.provenance),
        ) {
            Ok(document) => Some(document),
            Err(_) => {
                self.diagnostics.push(diagnostic_error(
                    "scene.invalid-provenance",
                    "Expanded scene source provenance is inconsistent",
                ));
                None
            }
        }
    }

    fn admit_terminal_work(
        &mut self,
        nodes: usize,
        bytes: usize,
        allocation: Option<usize>,
    ) -> Result<(), ()> {
        admit_prefab_terminal_work(
            self.options.limits,
            &mut self.usage,
            &mut self.reference_work,
            nodes,
            bytes,
            allocation,
        )
        .map_err(|(kind, observed, maximum)| {
            self.fail_budget(kind, observed, maximum, None, None);
        })
    }

    fn allocate_projected_entity_ids(
        &mut self,
        document: &SceneDraft,
    ) -> Option<BTreeMap<SceneEntityId, SceneEntityId>> {
        let mut entity_ids = BTreeMap::new();
        for entity in &document.entities {
            loop {
                let ordinal = self.next_projected_entity;
                let identifier_len = "prefab_".len()
                    + ordinal
                        .checked_ilog10()
                        .map_or(1, |digits| digits as usize + 1);
                let mut generated_bytes = 0;
                if let Err(error) = accumulate_generated_identifier_bytes(
                    self.usage.generated_identifier_bytes,
                    &mut generated_bytes,
                    identifier_len,
                    self.options
                        .limits
                        .single_generated_identifier_bytes()
                        .get(),
                    self.options.limits.generated_identifier_bytes().get(),
                ) {
                    self.fail_prefab_remap_budget(error, None);
                    return None;
                }
                self.next_projected_entity = ordinal
                    .checked_add(1)
                    .expect("in-memory entity allocation cannot exhaust usize ordinals");
                let mut next_usage = self.usage;
                next_usage.generated_identifier_bytes = next_usage
                    .generated_identifier_bytes
                    .checked_add(generated_bytes)?;
                if !self.admit_working_usage(next_usage, None, None) {
                    return None;
                }
                let candidate = SceneEntityId::new(format!("prefab_{ordinal}"))
                    .expect("a generated prefab locator is a valid scene entity ID");
                if self.reserved_entity_ids.insert(candidate.clone()) {
                    self.usage.generated_identifier_bytes += generated_bytes;
                    entity_ids.insert(entity.id.clone(), candidate);
                    break;
                }
            }
        }
        Some(entity_ids)
    }

    fn reserve_resolved_instance(&mut self, anchor_id: &SceneEntityId, source: &AssetRef) -> bool {
        let Some(instances) = self.observe_item_budget(
            SceneCompilationBudgetKind::ResolvedInstances,
            self.usage.resolved_instances,
            1,
            self.options.limits.resolved_instances(),
            Some(anchor_id),
            Some(source),
        ) else {
            return false;
        };
        let mut next_usage = self.usage;
        next_usage.resolved_instances = instances;
        if !self.admit_working_usage(next_usage, Some(anchor_id), Some(source)) {
            return false;
        }
        self.usage.resolved_instances = instances;
        true
    }

    fn reserve_provenance_bytes(
        &mut self,
        amount: Option<usize>,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> bool {
        let limit = self.options.limits.provenance_bytes();
        let Some(amount) =
            amount.filter(|amount| self.usage.provenance_bytes.checked_add(*amount).is_some())
        else {
            self.fail_budget(
                SceneCompilationBudgetKind::ProvenanceBytes,
                usize::MAX,
                limit.get(),
                entity,
                source,
            );
            return false;
        };
        let Some(bytes) = self.observe_byte_budget(
            SceneCompilationBudgetKind::ProvenanceBytes,
            self.usage.provenance_bytes,
            amount,
            limit,
            entity,
            source,
        ) else {
            return false;
        };
        let mut next_usage = self.usage;
        next_usage.provenance_bytes = bytes;
        if !self.admit_working_usage(next_usage, entity, source) {
            return false;
        }
        self.usage.provenance_bytes = bytes;
        true
    }

    fn working_set(&self) -> SceneCompilationWorkingSet {
        SceneCompilationWorkingSet {
            bytes: self.usage.checked_work_bytes(),
            working_limit: self.options.limits.working_bytes(),
            source_depth: 1,
        }
    }

    fn admit_working_usage(
        &mut self,
        usage: SceneCompilationUsage,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> bool {
        let bytes = usage.checked_work_bytes();
        let maximum = self.options.limits.working_bytes().get();
        if bytes.is_none_or(|bytes| bytes > maximum) {
            self.fail_budget(
                SceneCompilationBudgetKind::WorkingBytes,
                bytes.unwrap_or(usize::MAX),
                maximum,
                entity,
                source,
            );
            return false;
        }
        true
    }

    fn reserve_scratch_bytes(
        &mut self,
        bytes: Option<usize>,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> bool {
        let mut usage = self.usage;
        let Some(bytes) = bytes.and_then(|bytes| usage.scratch_bytes.checked_add(bytes)) else {
            self.fail_budget(
                SceneCompilationBudgetKind::WorkingBytes,
                usize::MAX,
                self.options.limits.working_bytes().get(),
                entity,
                source,
            );
            return false;
        };
        usage.scratch_bytes = bytes;
        if !self.admit_working_usage(usage, entity, source) {
            return false;
        }
        self.usage.scratch_bytes = bytes;
        true
    }

    fn reserve_raw_clone(
        &mut self,
        cost: crate::patch::ScenePatchCost,
        roots: &[SceneEntityId],
        anchor: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> bool {
        let bytes = cost
            .operation_count()
            .checked_mul(512)
            .and_then(|bytes| bytes.checked_add(cost.value_cost().nodes().checked_mul(96)?))
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
            .and_then(|bytes| {
                roots.iter().try_fold(bytes, |bytes, root| {
                    bytes
                        .checked_add(std::mem::size_of::<SceneEntityId>())?
                        .checked_add(crate::provenance::provenance_string_bytes(root.as_str())?)
                })
            });
        self.reserve_scratch_bytes(bytes, anchor, source)
    }

    fn materialization_limits(&self) -> ComponentValueMaterializationLimits {
        self.options.limits.materialization_limits()
    }

    fn fail_prefab_remap_budget(
        &mut self,
        error: PrefabReferenceRemapError,
        entity: Option<&SceneEntityId>,
    ) -> bool {
        let (kind, observed, maximum) = match error {
            PrefabReferenceRemapError::SingleGeneratedIdentifier { observed, maximum } => (
                SceneCompilationBudgetKind::SingleGeneratedIdentifierBytes,
                observed,
                maximum,
            ),
            PrefabReferenceRemapError::GeneratedIdentifier { observed, maximum } => (
                SceneCompilationBudgetKind::GeneratedIdentifierBytes,
                observed,
                maximum,
            ),
            PrefabReferenceRemapError::MaterializedValue { observed, maximum } => (
                SceneCompilationBudgetKind::MaterializedValueBytes,
                observed,
                maximum,
            ),
            PrefabReferenceRemapError::MissingSourceEntity => return false,
        };
        self.fail_budget(kind, observed, maximum, entity, None);
        true
    }

    fn push_prefab_reference_rewrite_error(
        &mut self,
        entity_id: &SceneEntityId,
        component_id: &str,
        error: &ComponentEntityReferenceRewriteError<PrefabReferenceRemapError>,
    ) {
        self.diagnostics.push(with_entity_reference_rewrite_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.prefab-entity-reference-rewrite-failed",
                        "Prefab entity reference rewrite failed before publication",
                    ),
                    "entity-id",
                    entity_id.as_str(),
                ),
                "component-id",
                component_id,
            ),
            error,
            prefab_reference_remap_error_name,
        ));
    }

    fn preflight_materialized_value_cost(
        &mut self,
        original_nodes: usize,
        replacement_nodes: usize,
        original_bytes: usize,
        replacement_bytes: usize,
        anchor_id: &SceneEntityId,
    ) -> Option<(usize, usize)> {
        let node_limit = self.options.limits.materialized_value_nodes().get();
        let byte_limit = self.options.limits.materialized_value_bytes().get();
        let Some(nodes) = self
            .usage
            .materialized_value_nodes
            .checked_sub(original_nodes)
            .and_then(|value| value.checked_add(replacement_nodes))
        else {
            self.fail_budget(
                SceneCompilationBudgetKind::MaterializedValueNodes,
                usize::MAX,
                node_limit,
                Some(anchor_id),
                None,
            );
            return None;
        };
        let Some(bytes) = self
            .usage
            .materialized_value_bytes
            .checked_sub(original_bytes)
            .and_then(|value| value.checked_add(replacement_bytes))
        else {
            self.fail_budget(
                SceneCompilationBudgetKind::MaterializedValueBytes,
                usize::MAX,
                byte_limit,
                Some(anchor_id),
                None,
            );
            return None;
        };
        if nodes > node_limit {
            self.fail_budget(
                SceneCompilationBudgetKind::MaterializedValueNodes,
                nodes,
                node_limit,
                Some(anchor_id),
                None,
            );
            return None;
        }
        if bytes > byte_limit {
            self.fail_budget(
                SceneCompilationBudgetKind::MaterializedValueBytes,
                bytes,
                byte_limit,
                Some(anchor_id),
                None,
            );
            return None;
        }

        let mut next_usage = self.usage;
        next_usage.materialized_value_nodes = nodes;
        next_usage.materialized_value_bytes = bytes;
        if !self.admit_working_usage(next_usage, Some(anchor_id), None) {
            return None;
        }
        Some((nodes, bytes))
    }

    fn observe_item_budget(
        &mut self,
        kind: SceneCompilationBudgetKind,
        current: usize,
        amount: usize,
        limit: ItemLimit,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> Option<usize> {
        let observed = current.saturating_add(amount);
        if observed > limit.get() {
            self.fail_budget(kind, observed, limit.get(), entity, source);
            return None;
        }
        Some(observed)
    }

    fn observe_byte_budget(
        &mut self,
        kind: SceneCompilationBudgetKind,
        current: usize,
        amount: usize,
        limit: ByteLimit,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> Option<usize> {
        let observed = current.saturating_add(amount);
        if observed > limit.get() {
            self.fail_budget(kind, observed, limit.get(), entity, source);
            return None;
        }
        Some(observed)
    }

    fn fail_budget(
        &mut self,
        kind: SceneCompilationBudgetKind,
        observed: usize,
        maximum: usize,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) {
        if self.budget_failed {
            return;
        }
        self.budget_failed = true;
        self.diagnostics.push(prefab_expansion_budget_diagnostic(
            kind, observed, maximum, entity, source,
        ));
    }

    #[cfg(feature = "serde")]
    fn validate_authoring_document(&mut self, document: &SceneDraft) -> DiagnosticReport {
        // Final values have already paid their materialized counters during lowering. This
        // guard admits validation's complete temporary copy, including IDs and record metadata,
        // against working storage rather than imposing value-only ceilings on that metadata.
        let working = self.options.limits.working_bytes();
        let cost = crate::patch::bounded_component_records_source_cost(
            document
                .entities
                .iter()
                .flat_map(|entity| entity.components.values()),
            crate::ScenePatchApplyLimits::default()
                .with_validation_value_nodes(
                    ItemLimit::new((working.get() / 96).max(1))
                        .expect("a working allowance produces a nonzero node ceiling"),
                )
                .with_validation_value_bytes(working),
        );
        let Some(cost) = cost else {
            self.fail_budget(
                SceneCompilationBudgetKind::WorkingBytes,
                usize::MAX,
                self.options.limits.working_bytes().get(),
                None,
                None,
            );
            return DiagnosticReport::default();
        };
        if !self.reserve_raw_clone(cost, &document.roots, None, None) {
            return DiagnosticReport::default();
        }
        let structure_bytes = document.entities.iter().try_fold(0usize, |bytes, entity| {
            entity.children.iter().try_fold(
                bytes
                    .checked_add(std::mem::size_of::<ExpandedSceneEntity>())?
                    .checked_add(crate::provenance::provenance_string_bytes(
                        entity.id.as_str(),
                    )?)?,
                |bytes, child| {
                    bytes.checked_add(crate::provenance::provenance_string_bytes(child.as_str())?)
                },
            )
        });
        if !self.reserve_scratch_bytes(structure_bytes, None, None) {
            return DiagnosticReport::default();
        }
        let mut context = ComponentDecodeContext::new()
            .with_materialization_limits(self.materialization_limits());
        if let Some(asset_resolution) = self.asset_resolution {
            context = context.with_asset_resolution(asset_resolution);
        }
        let authority = self.authority;
        let result = crate::validation::validate_expanded_values(
            document,
            authority,
            &mut context,
            |nodes, bytes, allocation| self.admit_terminal_work(nodes, bytes, Some(allocation)),
        );
        let Ok(result) = result else {
            return DiagnosticReport::default();
        };
        #[cfg(feature = "serde")]
        if matches!(self.authority, SchemaAuthority::Authoring(_))
            && !result.diagnostics.has_errors()
        {
            self.asset_references = result.asset_references;
        }
        result.diagnostics
    }

    #[cfg(feature = "serde")]
    fn observe_authoring_omissions(&mut self, document: &mut SceneDraft) {
        let SchemaAuthority::Authoring(basis) = self.authority else {
            return;
        };
        for entity in &mut document.entities {
            entity.components.retain(|component, record| {
                let unavailable = self.authority.unavailable_reason(component, record);
                let reason = unavailable
                    .map(AuthoringSceneCompilationOmissionReason::RecordUnavailable)
                    .or_else(|| {
                        (!basis.has_native_preview_adapter(component)).then_some(
                            AuthoringSceneCompilationOmissionReason::NativeBindingUnavailable,
                        )
                    });
                if let Some(reason) = reason {
                    let bytes = native_preview::omission_work_bytes(
                        &entity.id,
                        component,
                        self.provenance.get(&entity.id),
                    );
                    if !self.reserve_scratch_bytes(bytes, Some(&entity.id), None) {
                        return true;
                    }
                    self.omissions.push(AuthoringSceneCompilationOmission {
                        entity: entity.id.clone(),
                        component: component.clone(),
                        reason,
                    });
                }
                unavailable.is_none()
            });
        }
    }
}

fn prefab_expansion_budget_diagnostic(
    kind: SceneCompilationBudgetKind,
    observed: usize,
    maximum: usize,
    entity: Option<&SceneEntityId>,
    source: Option<&AssetRef>,
) -> nara_diagnostic::Diagnostic {
    let mut diagnostic = with_public_u64(
        with_public_u64(
            with_public_locator(
                diagnostic_error(
                    "scene.prefab-expansion-budget-exceeded",
                    "Prefab expansion budget was exceeded",
                ),
                "budget-kind",
                kind.as_str(),
            ),
            "observed",
            usize_to_u64(observed),
        ),
        "maximum",
        usize_to_u64(maximum),
    );
    if let Some(entity) = entity {
        diagnostic = with_public_locator(diagnostic, "entity-id", entity.as_str());
    }
    if let Some(source) = source {
        diagnostic = with_asset_ref(diagnostic, "asset-ref", source);
    }
    diagnostic
}

fn accumulate_generated_identifier_bytes(
    current: usize,
    pending: &mut usize,
    identifier_len: usize,
    single_limit: usize,
    aggregate_limit: usize,
) -> Result<(), PrefabReferenceRemapError> {
    if identifier_len > single_limit {
        return Err(PrefabReferenceRemapError::SingleGeneratedIdentifier {
            observed: identifier_len,
            maximum: single_limit,
        });
    }
    let Some(candidate_pending) = pending.checked_add(identifier_len) else {
        return Err(PrefabReferenceRemapError::GeneratedIdentifier {
            observed: usize::MAX,
            maximum: aggregate_limit,
        });
    };
    let Some(observed) = current.checked_add(candidate_pending) else {
        return Err(PrefabReferenceRemapError::GeneratedIdentifier {
            observed: usize::MAX,
            maximum: aggregate_limit,
        });
    };
    if observed > aggregate_limit {
        return Err(PrefabReferenceRemapError::GeneratedIdentifier {
            observed,
            maximum: aggregate_limit,
        });
    }
    *pending = candidate_pending;
    Ok(())
}

const fn prefab_reference_remap_error_name(error: &PrefabReferenceRemapError) -> &'static str {
    match error {
        PrefabReferenceRemapError::SingleGeneratedIdentifier { .. } => {
            "single-generated-identifier-bytes"
        }
        PrefabReferenceRemapError::GeneratedIdentifier { .. } => "generated-identifier-bytes",
        PrefabReferenceRemapError::MaterializedValue { .. } => "materialized-value-bytes",
        PrefabReferenceRemapError::MissingSourceEntity => "missing-source-entity",
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;

    fn source_key(name: &str) -> SourceDocumentKey {
        SourceDocumentKey::named(
            "6b691227-a68a-4b11-8d1c-a9d5d8c7e952".parse().unwrap(),
            name,
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn document(entity: &str) -> PrefabDocument {
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new(entity).unwrap(),
        )])
    }

    #[test]
    fn expansion_report_borrows_the_document_and_complete_provenance_together() {
        let registry = nara_reflect::ComponentRegistryBuilder::new()
            .freeze()
            .unwrap();
        let request = AssetRef::path("prefabs/hero.ron").unwrap();
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            request.clone(),
            source_key("characters.hero"),
            document("hero"),
        );
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(PrefabInstance::forest(request));
        let report = SceneDocument::from_ordered_roots([anchor]).compile(&registry, &resolver);

        assert!(!report.diagnostics.has_errors());
        let compiled: &CompiledSceneDocument = report.candidate.as_ref().unwrap();
        assert!(std::ptr::eq(report.document().unwrap(), compiled));
        assert!(std::ptr::eq(
            report.provenance().unwrap(),
            compiled.canonical_provenance()
        ));
        assert_eq!(compiled.entities().len(), 2);
        assert_eq!(compiled.canonical_provenance().len(), 2);
    }

    #[test]
    fn failed_expansion_report_exposes_neither_document_nor_provenance() {
        let registry = nara_reflect::ComponentRegistryBuilder::new()
            .freeze()
            .unwrap();
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(PrefabInstance::forest(
            AssetRef::path("prefabs/missing.ron").unwrap(),
        ));
        let report = SceneDocument::from_ordered_roots([anchor])
            .compile(&registry, &InMemoryPrefabSourceResolver::new());

        assert!(report.diagnostics.has_errors());
        assert!(report.candidate.is_none());
        assert!(report.document().is_none());
        assert!(report.provenance().is_none());
    }

    #[test]
    fn canonical_source_replacement_updates_every_request_alias() {
        let path = AssetRef::path("prefabs/hero.ron").unwrap();
        let alias = AssetRef::path("aliases/hero.ron").unwrap();
        let key = source_key("characters.hero");
        let mut resolver = InMemoryPrefabSourceResolver::new();
        assert_eq!(
            resolver.insert(path.clone(), key.clone(), document("old")),
            None
        );
        assert_eq!(
            resolver.insert(alias.clone(), key.clone(), document("new")),
            Some(document("old"))
        );

        let first = resolver.resolve_prefab(&path).unwrap();
        let second = resolver.resolve_prefab(&alias).unwrap();
        assert_eq!(first.key(), &key);
        assert_eq!(second.key(), &key);
        assert!(std::ptr::eq(first.document(), second.document()));
        assert_eq!(first.document(), &document("new"));
        assert_eq!(resolver.sources.len(), 1);
    }

    #[test]
    fn request_rebinding_keeps_other_aliases_and_releases_unaddressed_sources() {
        let path = AssetRef::path("prefabs/hero.ron").unwrap();
        let alias = AssetRef::path("aliases/hero.ron").unwrap();
        let first = source_key("characters.hero");
        let second = source_key("characters.enemy");
        let mut resolver = InMemoryPrefabSourceResolver::new()
            .with_prefab(path.clone(), first.clone(), document("hero"))
            .with_prefab(alias.clone(), first.clone(), document("hero"));

        assert_eq!(
            resolver.insert(path.clone(), second.clone(), document("enemy")),
            None
        );
        assert_eq!(resolver.resolve_prefab(&path).unwrap().key(), &second);
        assert_eq!(resolver.resolve_prefab(&alias).unwrap().key(), &first);
        assert_eq!(resolver.sources.len(), 2);

        assert_eq!(
            resolver.insert(alias.clone(), second.clone(), document("successor")),
            Some(document("enemy"))
        );
        assert_eq!(resolver.sources.len(), 1);
        assert!(!resolver.sources.contains_key(&first));
        assert_eq!(resolver.resolve_prefab(&alias).unwrap().key(), &second);
        assert_eq!(
            resolver.resolve_prefab(&path).unwrap().document(),
            &document("successor")
        );
        assert!(
            resolver
                .resolve_prefab(&AssetRef::path("missing.ron").unwrap())
                .is_none()
        );
    }
}
