use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    ComponentDecodeContext, ComponentEntityReferenceRewriteError, ComponentRegistry,
    ComponentValueCost, ComponentValueMaterializationLimits, EntityReferenceTraversalLimits,
};

use crate::{
    ExpandedSceneDocument, SceneDocument, SceneEntityId, SceneEntityProvenance, SceneEntityRecord,
    ScenePatchDocument, SourceDocumentKey,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_asset_ref,
        with_entity_reference_rewrite_error, with_public_locator, with_public_u64,
    },
    schema_authority::SchemaAuthority,
    validation::reference_traversal_limits,
};

#[cfg(feature = "serde")]
use crate::PrefabProjectionPath;
#[cfg(feature = "serde")]
use crate::provenance::provenance_string_bytes;

#[cfg(test)]
use crate::SourceDocumentKind;
mod declaration;
mod overlay;

pub use declaration::{
    PrefabDeclarationSource, PrefabExpansionFailure, PrefabExpansionFailureKind,
    PrefabInstanceLocation, PrefabInstancePath, PrefabInstancePathSegment, PrefabOverlaySlot,
    PrefabRevert,
};
#[cfg(feature = "serde")]
pub(crate) use overlay::cost::{PrefabOverlayCostError, PrefabOverlayCostKind};
pub(crate) use overlay::cost::{PrefabOverlayCostLimits, prefab_instance_cost};
pub use overlay::{
    PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, PrefabPlacement,
    PrefabPlacementParent, PrefabPlacementSlot, PrefabPropertyClaim, PrefabPropertyIntent,
    PrefabReferenceBinding,
};

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabDocument {
    pub roots: Vec<SceneEntityId>,
    pub entities: Vec<SceneEntityRecord>,
}

impl PrefabDocument {
    #[must_use]
    pub fn new(
        roots: impl IntoIterator<Item = SceneEntityId>,
        entities: impl IntoIterator<Item = SceneEntityRecord>,
    ) -> Self {
        let mut entities = entities.into_iter().collect::<Vec<_>>();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            roots: roots.into_iter().collect(),
            entities,
        }
    }

    /// Creates a flat prefab whose input record order is its root order.
    #[must_use]
    pub fn from_ordered_roots(entities: impl IntoIterator<Item = SceneEntityRecord>) -> Self {
        let entities = entities.into_iter().collect::<Vec<_>>();
        let roots = entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>();
        Self::new(roots, entities)
    }

    pub fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }

    #[must_use]
    pub fn instantiate(&self) -> SceneDocument {
        let mut document = SceneDocument {
            roots: self.roots.clone(),
            entities: self.entities.clone(),
        };
        document.canonicalize();
        document
    }

    #[must_use]
    pub fn instantiate_with_patch(
        &self,
        registry: &ComponentRegistry,
        patch: &ScenePatchDocument,
    ) -> PrefabInstantiationReport {
        let mut document = self.instantiate();
        let patch_report = patch.apply_to_scene(&mut document, registry);
        PrefabInstantiationReport::from_patch_report(document, patch_report)
    }

    #[must_use]
    pub fn instantiate_with_patch_and_asset_database(
        &self,
        registry: &ComponentRegistry,
        patch: &ScenePatchDocument,
        database: &ProjectAssetDatabase,
    ) -> PrefabInstantiationReport {
        let mut document = self.instantiate();
        let patch_report =
            patch.apply_to_scene_with_asset_database(&mut document, registry, database);
        PrefabInstantiationReport::from_patch_report(document, patch_report)
    }

    #[must_use]
    pub fn validate(&self, registry: &ComponentRegistry) -> DiagnosticReport {
        self.instantiate().validate(registry)
    }

    #[must_use]
    pub fn validate_with_asset_database(
        &self,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> DiagnosticReport {
        self.instantiate()
            .validate_with_asset_database(registry, database)
    }

    #[must_use]
    pub fn expand_prefabs<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> PrefabExpansionReport {
        self.instantiate().expand_prefabs(registry, resolver)
    }

    #[must_use]
    pub fn expand_prefabs_with_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        self.instantiate()
            .expand_prefabs_with_asset_database(registry, resolver, database)
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
    pub source: AssetRef,
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
            #[serde(default)]
            overrides: Option<PrefabOverlay>,
        }

        let wire = PrefabInstanceWire::deserialize(deserializer)?;
        Ok(Self {
            source: wire.source,
            overrides: wire.overrides,
        })
    }
}

/// Resolves a lookup request to its admitted source identity and immutable document together.
///
/// Requests are lookup coordinates, not semantic identity. Aliases of the same source must return
/// the same key and document; distinct sources must not share a key. This applies equally to
/// code-authored and file-backed resolvers, including document-only expansion.
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
}

#[derive(Debug, Clone)]
enum ResolvedPrefabDocument {
    Native(Arc<PrefabDocument>),
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
            document: ResolvedPrefabDocument::Native(document),
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
            key,
        }
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        match &self.document {
            ResolvedPrefabDocument::Native(document) => document,
            #[cfg(feature = "serde")]
            ResolvedPrefabDocument::Authoring(document) => document.document(),
            #[cfg(feature = "serde")]
            ResolvedPrefabDocument::AuthoringSource { document, .. } => document,
        }
    }

    pub(crate) fn matches_authority(&self, authority: SchemaAuthority<'_>) -> bool {
        match (&self.document, authority) {
            (ResolvedPrefabDocument::Native(_), SchemaAuthority::Native(_)) => true,
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
        registry: &ComponentRegistry,
    ) -> Result<Option<PrefabDocument>, crate::SceneFilePublicationError> {
        Ok(self.insert(source, key, candidate.publish(registry)?.into_document()))
    }

    #[cfg(feature = "serde")]
    pub fn insert_file_candidate_with_asset_database(
        &mut self,
        source: AssetRef,
        key: SourceDocumentKey,
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<Option<PrefabDocument>, crate::SceneFilePublicationError> {
        let (prefab, _) = candidate.into_canonical_document(registry)?;
        let diagnostics = prefab
            .instantiate()
            .validate_authoring_with_asset_database(registry, database);
        if diagnostics.has_errors() {
            return Err(crate::SceneFilePublicationError::new(diagnostics));
        }
        Ok(self.insert(source, key, prefab))
    }
}

impl PrefabSourceResolver for InMemoryPrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource> {
        self.requests
            .iter()
            .find(|(candidate, _)| candidate == source)
            .and_then(|(_, key)| {
                self.sources.get_key_value(key).map(|(key, prefab)| {
                    ResolvedPrefabSource::bound(Arc::clone(prefab), key.clone())
                })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExpansionBudgetKind {
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

impl PrefabExpansionBudgetKind {
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

impl Display for PrefabExpansionBudgetKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabExpansionLimits {
    materialized_entities: ItemLimit,
    materialized_components: ItemLimit,
    materialized_value_nodes: ItemLimit,
    materialized_value_bytes: ByteLimit,
    resolved_instances: ItemLimit,
    overlay_declarations: ItemLimit,
    generated_identifier_bytes: ByteLimit,
    single_generated_identifier_bytes: ByteLimit,
    provenance_bytes: ByteLimit,
    working_bytes: ByteLimit,
    reference_work_nodes: ItemLimit,
    reference_work_bytes: ByteLimit,
}

impl Default for PrefabExpansionLimits {
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

impl PrefabExpansionLimits {
    /// Document-wide Schema, value and path work for effective asset-reference discovery.
    /// This includes unsuccessful/empty discovery and is separate from stored value size.
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabExpansionOptions {
    pub max_depth: usize,
    pub limits: PrefabExpansionLimits,
}

impl Default for PrefabExpansionOptions {
    fn default() -> Self {
        Self {
            max_depth: 32,
            limits: PrefabExpansionLimits::default(),
        }
    }
}

impl PrefabExpansionOptions {
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    #[must_use]
    pub const fn with_limits(mut self, limits: PrefabExpansionLimits) -> Self {
        self.limits = limits;
        self
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct PrefabExpansionReport {
    /// The immutable expanded document and its complete provenance, absent after failed expansion.
    ///
    /// Its source owner must bind the containing source before materialization. Expansion does
    /// not infer that source or claim a runtime occurrence.
    pub candidate: Option<ExpandedSceneDocument>,
    pub diagnostics: DiagnosticReport,
    /// The first rejected compiler step, independent of source Schema classification.
    pub failure: Option<PrefabExpansionFailure>,
    source_upgrade_required: bool,
}

/// Disposable observations from compilation against one admitted data-authoring basis.
#[cfg(feature = "serde")]
#[derive(Debug)]
pub struct AuthoringPrefabExpansionReport {
    pub candidate: Option<ExpandedSceneDocument>,
    pub diagnostics: DiagnosticReport,
    pub failure: Option<PrefabExpansionFailure>,
    pub omissions: Vec<AuthoringPrefabOmission>,
    asset_references: Vec<nara_reflect::DeclaredAssetReference>,
    work_bytes: Option<usize>,
    budget_failed: bool,
}

#[cfg(feature = "serde")]
impl AuthoringPrefabExpansionReport {
    pub const fn checked_work_bytes(&self) -> Option<usize> {
        self.work_bytes
    }

    pub const fn budget_exceeded(&self) -> bool {
        self.budget_failed
    }

    /// Actual effective values and defaults, before native preview component filtering.
    pub fn effective_asset_references(&self) -> &[nara_reflect::DeclaredAssetReference] {
        &self.asset_references
    }

    /// Consumes semantic compilation after asset capture, retaining the exact source provenance.
    pub fn into_native_preview(
        self,
    ) -> Result<
        (
            ExpandedSceneDocument,
            Vec<crate::authoring_preview::AuthoringPreviewOmission>,
        ),
        DiagnosticReport,
    > {
        let Some(candidate) = self.candidate else {
            return Err(self.diagnostics);
        };
        let candidate = candidate.remove_compiler_components(
            self.omissions
                .iter()
                .filter(|fact| {
                    fact.reason == AuthoringPrefabOmissionReason::NativeBindingUnavailable
                })
                .map(|fact| (&fact.entity, &fact.component)),
        );
        let omissions = self
            .omissions
            .iter()
            .map(|fact| {
                crate::authoring_preview::AuthoringPreviewOmission::from_expansion(fact, &candidate)
            })
            .collect();
        Ok((candidate, omissions))
    }
}

#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringPrefabOmission {
    /// Projected locator in this report's immutable candidate; consult its typed provenance.
    pub entity: SceneEntityId,
    pub component: nara_reflect::ComponentTypeId,
    pub reason: AuthoringPrefabOmissionReason,
}

#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringPrefabOmissionReason {
    RecordUnavailable(crate::AuthoringRecordReadiness),
    NativeBindingUnavailable,
}

/// Compiles the same selected declaration program without native codecs or migrations.
/// Source publication, saving and history remain the containing source owner's responsibility.
#[cfg(feature = "serde")]
pub fn try_expand_authoring_prefabs_with_loader<F, E>(
    document: &SceneDocument,
    basis: &crate::AuthoringSchemaBasis,
    options: PrefabExpansionOptions,
    database: Option<&ProjectAssetDatabase>,
    mut loader: F,
) -> Result<AuthoringPrefabExpansionReport, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut context = PrefabExpansionContext::new(
        SchemaAuthority::Authoring(basis),
        &mut loader,
        options,
        database,
    );
    let report = context.expand_scene_document(document);
    let work_bytes = PrefabExpansionContext::<F, E>::usage_work_bytes(context.usage);
    if let Some(error) = context.load_error {
        return Err(error);
    }
    Ok(AuthoringPrefabExpansionReport {
        candidate: report.candidate,
        diagnostics: report.diagnostics,
        failure: report.failure,
        omissions: context.omissions,
        asset_references: context.asset_references,
        work_bytes,
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
    pub(crate) entities: Vec<SceneEntityRecord>,
    pub(crate) provenance: crate::SceneEntityProvenanceMap,
}

#[cfg(feature = "serde")]
#[derive(Debug)]
pub(crate) struct SelectedInstanceCompilation {
    pub(crate) fragment: Option<SelectedInstanceFragment>,
    pub(crate) diagnostics: DiagnosticReport,
    pub(crate) failure: Option<PrefabExpansionFailure>,
    pub(crate) omissions: Vec<AuthoringPrefabOmission>,
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
    basis: &crate::AuthoringSchemaBasis,
    options: PrefabExpansionOptions,
    database: Option<&ProjectAssetDatabase>,
    mut loader: F,
) -> Result<SelectedInstanceCompilation, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut context = PrefabExpansionContext::new(
        SchemaAuthority::Authoring(basis),
        &mut loader,
        options,
        database,
    );
    let result: Result<SelectedInstanceFragment, PrefabExpansionFailure> = (|| {
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
            return Err(PrefabExpansionFailureKind::WorkLimit.into());
        }
        let mut document =
            overlay::compile::expand_selected_instance(&mut context, source, anchor)?;
        document.canonicalize();
        context.observe_authoring_omissions(&mut document);
        if !context.diagnostics.has_errors() {
            // The staged layout retains real containing-source shells. Their IDs close
            // references, but they have no component programs in selected demand.
            let validation = context.validate_document(&document);
            context.diagnostics.extend(validation);
        }
        if context.diagnostics.has_errors() {
            return Err(if context.budget_failed {
                PrefabExpansionFailureKind::WorkLimit
            } else {
                PrefabExpansionFailureKind::OwnerAdmission
            }
            .into());
        }
        context.provenance.retain(|_, origin| {
            origin
                .projection_path()
                .is_some_and(|path| path.root_anchor() == anchor)
        });
        let record = document
            .entities
            .iter_mut()
            .find(|record| &record.id == anchor)
            .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?;
        // The existing layout owner puts projected roots before ordinary local children.
        let mut roots = std::mem::take(&mut record.children);
        roots.retain(|id| context.provenance.contains_key(id));
        document
            .entities
            .retain(|record| context.provenance.contains_key(&record.id));
        Ok(SelectedInstanceFragment {
            roots,
            entities: document.entities,
            provenance: std::mem::take(&mut context.provenance),
        })
    })();
    let (fragment, failure) = match result {
        Ok(fragment) => (Some(fragment), None),
        Err(failure) => {
            context.push_expansion_failure(&failure);
            (None, Some(failure))
        }
    };
    let work_bytes = PrefabExpansionContext::<F, E>::usage_work_bytes(context.usage);
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

impl PrefabExpansionReport {
    /// Whether a retained source component was migrated during successful compilation.
    /// Discarded component payloads do not contribute to this observation.
    #[must_use]
    pub const fn source_upgrade_required(&self) -> bool {
        self.source_upgrade_required
    }

    /// Borrows the expanded document without granting materialization authority by itself.
    #[must_use]
    pub fn document(&self) -> Option<&SceneDocument> {
        self.candidate.as_ref().map(ExpandedSceneDocument::document)
    }

    /// Borrows the source association established by this expansion.
    #[must_use]
    pub fn provenance(&self) -> Option<&crate::SceneEntityProvenanceMap> {
        self.candidate
            .as_ref()
            .map(ExpandedSceneDocument::provenance)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PrefabExpansionUsage {
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

/// Conservative modeled allocation held by a suspended expansion, excluding resolver caches.
/// Raw clones and derived scratch are cumulatively charged without refunds to private work.
#[derive(Debug, Clone, Copy)]
pub struct PrefabExpansionWorkingSet {
    bytes: Option<usize>,
    working_limit: ByteLimit,
    source_depth: usize,
}

impl PrefabExpansionWorkingSet {
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

impl PrefabExpansionLimits {
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
    pub fn expand_prefabs<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
    ) -> PrefabExpansionReport {
        self.expand_prefabs_with_options(registry, resolver, PrefabExpansionOptions::default())
    }

    #[must_use]
    pub fn expand_prefabs_with_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        self.expand_prefabs_with_options_and_asset_database(
            registry,
            resolver,
            PrefabExpansionOptions::default(),
            database,
        )
    }

    #[must_use]
    pub fn expand_prefabs_with_options<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        options: PrefabExpansionOptions,
    ) -> PrefabExpansionReport {
        match self.try_expand_prefabs_with_loader(registry, options, None, |source, _| {
            Ok::<_, Infallible>(resolver.resolve_prefab(source))
        }) {
            Ok(report) => report,
            Err(error) => match error {},
        }
    }

    #[must_use]
    pub fn expand_prefabs_with_options_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        options: PrefabExpansionOptions,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        match self.try_expand_prefabs_with_loader(registry, options, Some(database), |source, _| {
            Ok::<_, Infallible>(resolver.resolve_prefab(source))
        }) {
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
    pub fn try_expand_prefabs_with_loader<F, E>(
        &self,
        registry: &ComponentRegistry,
        options: PrefabExpansionOptions,
        database: Option<&ProjectAssetDatabase>,
        mut loader: F,
    ) -> Result<PrefabExpansionReport, E>
    where
        F: FnMut(
            &AssetRef,
            &mut PrefabExpansionWorkingSet,
        ) -> Result<Option<ResolvedPrefabSource>, E>,
    {
        let mut context = PrefabExpansionContext::new(
            SchemaAuthority::Native(registry),
            &mut loader,
            options,
            database,
        );
        let report = context.expand_scene_document(self);
        match context.load_error {
            Some(error) => Err(error),
            None => Ok(report),
        }
    }
}

struct PrefabExpansionContext<'a, F, E> {
    authority: SchemaAuthority<'a>,
    resolver: &'a mut F,
    load_error: Option<E>,
    options: PrefabExpansionOptions,
    database: Option<&'a ProjectAssetDatabase>,
    usage: PrefabExpansionUsage,
    budget_failed: bool,
    source_upgrade_required: bool,
    diagnostics: DiagnosticReport,
    provenance: BTreeMap<SceneEntityId, SceneEntityProvenance>,
    reserved_entity_ids: BTreeSet<SceneEntityId>,
    next_projected_entity: usize,
    #[cfg(feature = "serde")]
    asset_references: Vec<nara_reflect::DeclaredAssetReference>,
    #[cfg(feature = "serde")]
    omissions: Vec<AuthoringPrefabOmission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefabReferenceRemapError {
    SingleGeneratedIdentifier { observed: usize, maximum: usize },
    GeneratedIdentifier { observed: usize, maximum: usize },
    MaterializedValue { observed: usize, maximum: usize },
    MissingSourceEntity,
}

impl<'a, F, E> PrefabExpansionContext<'a, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    fn new(
        authority: SchemaAuthority<'a>,
        resolver: &'a mut F,
        options: PrefabExpansionOptions,
        database: Option<&'a ProjectAssetDatabase>,
    ) -> Self {
        Self {
            authority,
            resolver,
            load_error: None,
            options,
            database,
            usage: PrefabExpansionUsage::default(),
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

    fn expand_scene_document(&mut self, document: &SceneDocument) -> PrefabExpansionReport {
        if !self.reserve_provenance_bytes(
            Some(std::mem::size_of::<crate::SceneEntityProvenanceMap>()),
            None,
            None,
        ) {
            return PrefabExpansionReport {
                candidate: None,
                diagnostics: std::mem::take(&mut self.diagnostics),
                failure: Some(PrefabExpansionFailureKind::WorkLimit.into()),
                source_upgrade_required: false,
            };
        }
        let mut expanded = match overlay::compile::expand_document(self, document) {
            Ok(document) => document,
            Err(failure) => {
                self.push_expansion_failure(&failure);
                return PrefabExpansionReport {
                    candidate: None,
                    diagnostics: std::mem::take(&mut self.diagnostics),
                    failure: Some(failure),
                    source_upgrade_required: false,
                };
            }
        };
        expanded.canonicalize();

        #[cfg(feature = "serde")]
        if !self.diagnostics.has_errors() {
            self.observe_authoring_omissions(&mut expanded);
        }

        if !self.diagnostics.has_errors() {
            let validation = self.validate_document(&expanded);
            self.diagnostics.extend(validation);
        }

        if self.diagnostics.has_errors() {
            return PrefabExpansionReport {
                candidate: None,
                diagnostics: std::mem::take(&mut self.diagnostics),
                failure: None,
                source_upgrade_required: false,
            };
        }

        let candidate =
            ExpandedSceneDocument::from_expansion(expanded, std::mem::take(&mut self.provenance));
        let candidate = match candidate {
            Ok(candidate) => Some(candidate),
            Err(_) => {
                self.diagnostics.push(diagnostic_error(
                    "scene.invalid-provenance",
                    "Expanded scene source provenance is inconsistent",
                ));
                None
            }
        };
        PrefabExpansionReport {
            candidate,
            diagnostics: std::mem::take(&mut self.diagnostics),
            failure: None,
            source_upgrade_required: self.source_upgrade_required,
        }
    }

    fn allocate_projected_entity_ids(
        &mut self,
        document: &SceneDocument,
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
            PrefabExpansionBudgetKind::ResolvedInstances,
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
                PrefabExpansionBudgetKind::ProvenanceBytes,
                usize::MAX,
                limit.get(),
                entity,
                source,
            );
            return false;
        };
        let Some(bytes) = self.observe_byte_budget(
            PrefabExpansionBudgetKind::ProvenanceBytes,
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

    fn working_set(&self) -> PrefabExpansionWorkingSet {
        PrefabExpansionWorkingSet {
            bytes: Self::usage_work_bytes(self.usage),
            working_limit: self.options.limits.working_bytes(),
            source_depth: 1,
        }
    }

    fn usage_work_bytes(usage: PrefabExpansionUsage) -> Option<usize> {
        expansion_work_bytes(
            [
                usage.materialized_entities,
                usage.materialized_components,
                usage.materialized_value_nodes,
                usage.resolved_instances,
                usage.overlay_declarations,
            ],
            usage.materialized_value_bytes,
            usage.generated_identifier_bytes,
            usage.provenance_bytes,
        )?
        .checked_add(usage.scratch_bytes)
    }

    fn admit_working_usage(
        &mut self,
        usage: PrefabExpansionUsage,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) -> bool {
        let bytes = Self::usage_work_bytes(usage);
        let maximum = self.options.limits.working_bytes().get();
        if bytes.is_none_or(|bytes| bytes > maximum) {
            self.fail_budget(
                PrefabExpansionBudgetKind::WorkingBytes,
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
                PrefabExpansionBudgetKind::WorkingBytes,
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

    fn reserve_component_temporary(
        &mut self,
        schema: &nara_reflect::ComponentSchema,
        value: ComponentValueCost,
        entity: Option<&SceneEntityId>,
    ) -> bool {
        // Effective/default and bound values can coexist with their raw source. Pay for the
        // whole temporary value before the existing Reflect materializer copies defaults.
        let bytes = value
            .nodes()
            .checked_mul(96)
            .and_then(|bytes| bytes.checked_add(value.logical_bytes()))
            .and_then(|bytes| {
                schema
                    .fields
                    .iter()
                    .filter_map(|field| field.default_value.as_ref())
                    .try_fold(bytes, |bytes, default| {
                        let cost = default.cost_up_to(
                            self.options.limits.materialized_value_nodes().get(),
                            self.options.limits.materialized_value_bytes().get(),
                        )?;
                        bytes.checked_add(
                            cost.nodes()
                                .checked_mul(96)?
                                .checked_add(cost.logical_bytes())?
                                .checked_mul(3)?,
                        )
                    })
            });
        self.reserve_scratch_bytes(bytes, entity, None)
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

    fn prefab_reference_traversal_limits(&self) -> EntityReferenceTraversalLimits {
        reference_traversal_limits(self.materialization_limits())
    }

    fn materialization_limits(&self) -> ComponentValueMaterializationLimits {
        ComponentValueMaterializationLimits::new(
            self.options.limits.materialized_value_nodes(),
            self.options.limits.materialized_value_bytes(),
            EntityReferenceTraversalLimits::default().depth(),
        )
    }

    fn fail_prefab_remap_budget(
        &mut self,
        error: PrefabReferenceRemapError,
        entity: Option<&SceneEntityId>,
    ) -> bool {
        let (kind, observed, maximum) = match error {
            PrefabReferenceRemapError::SingleGeneratedIdentifier { observed, maximum } => (
                PrefabExpansionBudgetKind::SingleGeneratedIdentifierBytes,
                observed,
                maximum,
            ),
            PrefabReferenceRemapError::GeneratedIdentifier { observed, maximum } => (
                PrefabExpansionBudgetKind::GeneratedIdentifierBytes,
                observed,
                maximum,
            ),
            PrefabReferenceRemapError::MaterializedValue { observed, maximum } => (
                PrefabExpansionBudgetKind::MaterializedValueBytes,
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
                PrefabExpansionBudgetKind::MaterializedValueNodes,
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
                PrefabExpansionBudgetKind::MaterializedValueBytes,
                usize::MAX,
                byte_limit,
                Some(anchor_id),
                None,
            );
            return None;
        };
        if nodes > node_limit {
            self.fail_budget(
                PrefabExpansionBudgetKind::MaterializedValueNodes,
                nodes,
                node_limit,
                Some(anchor_id),
                None,
            );
            return None;
        }
        if bytes > byte_limit {
            self.fail_budget(
                PrefabExpansionBudgetKind::MaterializedValueBytes,
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
        kind: PrefabExpansionBudgetKind,
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
        kind: PrefabExpansionBudgetKind,
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
        kind: PrefabExpansionBudgetKind,
        observed: usize,
        maximum: usize,
        entity: Option<&SceneEntityId>,
        source: Option<&AssetRef>,
    ) {
        if self.budget_failed {
            return;
        }
        self.budget_failed = true;
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
        self.diagnostics.push(diagnostic);
    }

    fn validate_document(&mut self, document: &SceneDocument) -> DiagnosticReport {
        let cost = crate::patch::bounded_scene_entities_source_cost(
            &document.entities,
            crate::ScenePatchApplyLimits::default()
                .with_validation_value_nodes(self.options.limits.materialized_value_nodes())
                .with_validation_value_bytes(self.options.limits.materialized_value_bytes()),
        );
        let Some(cost) = cost else {
            self.fail_budget(
                PrefabExpansionBudgetKind::WorkingBytes,
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
        let mut context = ComponentDecodeContext::new()
            .with_materialization_limits(self.materialization_limits());
        if let Some(database) = self.database {
            context = context.with_project_asset_database(database);
        }
        let reference_node_limit = self.options.limits.reference_work_nodes().get();
        let reference_byte_limit = self.options.limits.reference_work_bytes().get();
        let mut reference_nodes = 0_usize;
        let mut reference_bytes = 0_usize;
        let authority = self.authority;
        let result = crate::validation::validate_expanded_values(
            document,
            authority,
            &mut context,
            |nodes, bytes, allocation| {
                reference_nodes = reference_nodes.saturating_add(nodes);
                reference_bytes = reference_bytes.saturating_add(bytes);
                if reference_nodes > reference_node_limit {
                    self.fail_budget(
                        PrefabExpansionBudgetKind::ReferenceWorkNodes,
                        reference_nodes,
                        reference_node_limit,
                        None,
                        None,
                    );
                    return Err(());
                }
                if reference_bytes > reference_byte_limit {
                    self.fail_budget(
                        PrefabExpansionBudgetKind::ReferenceWorkBytes,
                        reference_bytes,
                        reference_byte_limit,
                        None,
                        None,
                    );
                    return Err(());
                }
                self.reserve_scratch_bytes(Some(allocation), None, None)
                    .then_some(())
                    .ok_or(())
            },
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
    fn observe_authoring_omissions(&mut self, document: &mut SceneDocument) {
        let SchemaAuthority::Authoring(basis) = self.authority else {
            return;
        };
        for entity in &mut document.entities {
            entity.components.retain(|component, record| {
                let unavailable = self.authority.unavailable_reason(component, record);
                let reason = unavailable
                    .map(AuthoringPrefabOmissionReason::RecordUnavailable)
                    .or_else(|| {
                        (!basis.has_native_preview_adapter(component))
                            .then_some(AuthoringPrefabOmissionReason::NativeBindingUnavailable)
                    });
                if let Some(reason) = reason {
                    // The retained fact and its later native-preview observation coexist.
                    // Reserve the complete typed ancestry before either result can copy it.
                    let bytes = provenance_string_bytes(entity.id.as_str())
                        .and_then(|bytes| {
                            bytes.checked_add(provenance_string_bytes(component.as_str())?)
                        })
                        .and_then(|bytes| bytes.checked_mul(2))
                        .and_then(|bytes| {
                            bytes.checked_add(
                                std::mem::size_of::<AuthoringPrefabOmission>()
                                    + std::mem::size_of::<
                                        crate::authoring_preview::AuthoringPreviewOmission,
                                    >(),
                            )
                        })
                        .and_then(|bytes| {
                            let origin = self.provenance.get(&entity.id)?;
                            let Some(path) = origin.projection_path() else {
                                return Some(bytes);
                            };
                            bytes
                                .checked_add(std::mem::size_of::<SceneEntityProvenance>())?
                                .checked_add(provenance_string_bytes(
                                    origin.local_entity_id().as_str(),
                                )?)?
                                .checked_add(PrefabProjectionPath::extended_allocation_bytes(
                                    Some(path),
                                    path.root_anchor(),
                                    0,
                                )?)
                        });
                    if !self.reserve_scratch_bytes(bytes, Some(&entity.id), None) {
                        return true;
                    }
                    self.omissions.push(AuthoringPrefabOmission {
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
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let request = AssetRef::path("prefabs/hero.ron").unwrap();
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            request.clone(),
            source_key("characters.hero"),
            document("hero"),
        );
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: request,
            overrides: None,
        });
        let report =
            SceneDocument::from_ordered_roots([anchor]).expand_prefabs(&registry, &resolver);

        assert!(!report.diagnostics.has_errors());
        let expanded: &ExpandedSceneDocument = report.candidate.as_ref().unwrap();
        assert!(std::ptr::eq(
            report.document().unwrap(),
            expanded.document()
        ));
        assert!(std::ptr::eq(
            report.provenance().unwrap(),
            expanded.provenance()
        ));
        assert_eq!(expanded.document().entities.len(), 2);
        assert_eq!(expanded.provenance().len(), 2);
    }

    #[test]
    fn failed_expansion_report_exposes_neither_document_nor_provenance() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("prefabs/missing.ron").unwrap(),
            overrides: None,
        });
        let report = SceneDocument::from_ordered_roots([anchor])
            .expand_prefabs(&registry, &InMemoryPrefabSourceResolver::new());

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
