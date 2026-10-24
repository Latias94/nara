use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_identity::EntityReference;
use nara_reflect::__private::{DeclaredEntityReferencePlan, plan_declared_entity_references};
use nara_reflect::{
    ComponentDecodeContext, ComponentEntityReferenceRewriteError, ComponentRegistry,
    ComponentValue, ComponentValueCost, ComponentValueMaterializationLimits,
    EntityReferenceTraversalLimits, effective_component_value_owned,
};

use crate::{
    ExpandedSceneCandidate, PrefabProjectionPath, SceneDocument, SceneEntityId,
    SceneEntityProvenance, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    SourceDocumentKey, SourceDocumentKind,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_asset_ref, with_codec_error,
        with_entity_reference_rewrite_error, with_migration_error, with_public_locator,
        with_public_u64,
    },
    patch::{
        scene_entities_source_value_cost, scene_entity_component_value_cost, scene_patch_value_cost,
    },
    provenance::{provenance_asset_ref_bytes, provenance_entry_bytes},
    validation::{
        preflight_scene_with_context, reference_traversal_limits, validate_scene_structure,
    },
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
    pub overrides: ScenePatchDocument,
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
            overrides: crate::patch::ScenePatchDocumentWire,
        }

        let wire = PrefabInstanceWire::deserialize(deserializer)?;
        Ok(Self {
            source: wire.source,
            overrides: wire.overrides.into_document(),
        })
    }
}

pub trait PrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument>;

    /// Resolves a request to one immutable document and its canonical source key when the
    /// admitting owner has one. Project-backed adapters override this method; the default keeps
    /// source-keyless in-memory adapters useful for document-only tests.
    fn resolve_prefab_binding(&self, source: &AssetRef) -> Option<ResolvedPrefabSource<'_>> {
        self.resolve_prefab(source)
            .map(ResolvedPrefabSource::unbound)
    }
}

/// One resolver-owned immutable Prefab and its canonical source identity, when admitted.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedPrefabSource<'a> {
    document: &'a PrefabDocument,
    key: Option<&'a SourceDocumentKey>,
}

impl<'a> ResolvedPrefabSource<'a> {
    pub const fn bound(document: &'a PrefabDocument, key: &'a SourceDocumentKey) -> Self {
        Self {
            document,
            key: Some(key),
        }
    }

    const fn unbound(document: &'a PrefabDocument) -> Self {
        Self {
            document,
            key: None,
        }
    }

    #[must_use]
    pub const fn document(self) -> &'a PrefabDocument {
        self.document
    }

    #[must_use]
    pub const fn key(self) -> Option<&'a SourceDocumentKey> {
        self.key
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InMemoryPrefabSourceResolver {
    sources: Vec<(AssetRef, PrefabDocument)>,
}

impl InMemoryPrefabSourceResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_prefab(mut self, source: AssetRef, prefab: PrefabDocument) -> Self {
        self.insert(source, prefab);
        self
    }

    pub fn insert(&mut self, source: AssetRef, prefab: PrefabDocument) -> Option<PrefabDocument> {
        if let Some((_, existing)) = self
            .sources
            .iter_mut()
            .find(|(candidate, _)| candidate == &source)
        {
            return Some(std::mem::replace(existing, prefab));
        }

        self.sources.push((source, prefab));
        None
    }

    #[cfg(feature = "serde")]
    pub fn insert_file_candidate(
        &mut self,
        source: AssetRef,
        candidate: crate::PrefabDocumentCandidate,
        registry: &ComponentRegistry,
    ) -> Result<Option<PrefabDocument>, crate::SceneFilePublicationError> {
        Ok(self.insert(source, candidate.publish(registry)?.into_document()))
    }

    #[cfg(feature = "serde")]
    pub fn insert_file_candidate_with_asset_database(
        &mut self,
        source: AssetRef,
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
        Ok(self.insert(source, prefab))
    }
}

impl PrefabSourceResolver for InMemoryPrefabSourceResolver {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        self.sources
            .iter()
            .find(|(candidate, _)| candidate == source)
            .map(|(_, prefab)| prefab)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExpansionBudgetKind {
    MaterializedEntities,
    MaterializedComponents,
    MaterializedValueNodes,
    MaterializedValueBytes,
    ResolvedInstances,
    AppliedPatchOperations,
    GeneratedIdentifierBytes,
    SingleGeneratedIdentifierBytes,
    ProvenanceBytes,
}

impl PrefabExpansionBudgetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MaterializedEntities => "materialized-entities",
            Self::MaterializedComponents => "materialized-components",
            Self::MaterializedValueNodes => "materialized-value-nodes",
            Self::MaterializedValueBytes => "materialized-value-bytes",
            Self::ResolvedInstances => "resolved-instances",
            Self::AppliedPatchOperations => "applied-patch-operations",
            Self::GeneratedIdentifierBytes => "generated-identifier-bytes",
            Self::SingleGeneratedIdentifierBytes => "single-generated-identifier-bytes",
            Self::ProvenanceBytes => "provenance-bytes",
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
    applied_patch_operations: ItemLimit,
    generated_identifier_bytes: ByteLimit,
    single_generated_identifier_bytes: ByteLimit,
    provenance_bytes: ByteLimit,
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
            applied_patch_operations: ItemLimit::new(100_000)
                .expect("prefab expansion patch operation limit is non-zero"),
            generated_identifier_bytes: ByteLimit::new(8 * 1024 * 1024)
                .expect("prefab expansion generated identifier byte limit is non-zero"),
            single_generated_identifier_bytes: ByteLimit::new(1024 * 1024)
                .expect("prefab expansion identifier byte limit is non-zero"),
            provenance_bytes: ByteLimit::new(64 * 1024 * 1024)
                .expect("prefab expansion provenance byte limit is non-zero"),
        }
    }
}

impl PrefabExpansionLimits {
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
    pub const fn applied_patch_operations(self) -> ItemLimit {
        self.applied_patch_operations
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
    /// Includes active stack entries and each owned copy of a projection chain, source key,
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
    pub const fn with_applied_patch_operations(mut self, limit: ItemLimit) -> Self {
        self.applied_patch_operations = limit;
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
    /// The immutable document and its source association, absent after failed expansion.
    pub candidate: Option<ExpandedSceneCandidate>,
    pub diagnostics: DiagnosticReport,
}

impl PrefabExpansionReport {
    /// Borrows the expanded document without granting materialization authority by itself.
    #[must_use]
    pub fn document(&self) -> Option<&SceneDocument> {
        self.candidate
            .as_ref()
            .map(ExpandedSceneCandidate::document)
    }

    /// Borrows the source association established by this expansion.
    #[must_use]
    pub fn provenance(&self) -> Option<&crate::SceneEntityProvenanceMap> {
        self.candidate
            .as_ref()
            .map(ExpandedSceneCandidate::provenance)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PrefabExpansionUsage {
    materialized_entities: usize,
    materialized_components: usize,
    materialized_value_nodes: usize,
    materialized_value_bytes: usize,
    resolved_instances: usize,
    applied_patch_operations: usize,
    generated_identifier_bytes: usize,
    provenance_bytes: usize,
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
        let mut context = PrefabExpansionContext::new(registry, resolver, options, None);
        context.expand_scene_document(self)
    }

    #[must_use]
    pub fn expand_prefabs_with_options_and_asset_database<R: PrefabSourceResolver + ?Sized>(
        &self,
        registry: &ComponentRegistry,
        resolver: &R,
        options: PrefabExpansionOptions,
        database: &ProjectAssetDatabase,
    ) -> PrefabExpansionReport {
        let mut context = PrefabExpansionContext::new(registry, resolver, options, Some(database));
        context.expand_scene_document(self)
    }
}

struct PrefabExpansionContext<'a, R: PrefabSourceResolver + ?Sized> {
    registry: &'a ComponentRegistry,
    resolver: &'a R,
    options: PrefabExpansionOptions,
    database: Option<&'a ProjectAssetDatabase>,
    stack: Vec<ActivePrefabSource>,
    usage: PrefabExpansionUsage,
    budget_failed: bool,
    diagnostics: DiagnosticReport,
    provenance: BTreeMap<SceneEntityId, SceneEntityProvenance>,
    reserved_entity_ids: BTreeSet<SceneEntityId>,
    next_projected_entity: usize,
}

#[derive(Debug)]
struct ActivePrefabSource {
    request: AssetRef,
    key: Option<SourceDocumentKey>,
}

impl ActivePrefabSource {
    fn allocation_bytes(request: &AssetRef, key: Option<&SourceDocumentKey>) -> Option<usize> {
        let key_bytes = match key {
            Some(key) => key.checked_retained_bytes()?,
            None => 0,
        };
        std::mem::size_of::<AssetRef>()
            .checked_add(std::mem::size_of::<Option<SourceDocumentKey>>())?
            .checked_add(provenance_asset_ref_bytes(request)?)?
            .checked_add(key_bytes)
    }

    fn matches(&self, request: &AssetRef, key: Option<&SourceDocumentKey>) -> bool {
        match (&self.key, key) {
            (Some(left), Some(right)) => left == right,
            _ => self.request == *request,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefabReferenceRemapError {
    SingleGeneratedIdentifier { observed: usize, maximum: usize },
    GeneratedIdentifier { observed: usize, maximum: usize },
    MaterializedValue { observed: usize, maximum: usize },
    MissingSourceEntity,
}

impl<'a, R: PrefabSourceResolver + ?Sized> PrefabExpansionContext<'a, R> {
    fn new(
        registry: &'a ComponentRegistry,
        resolver: &'a R,
        options: PrefabExpansionOptions,
        database: Option<&'a ProjectAssetDatabase>,
    ) -> Self {
        Self {
            registry,
            resolver,
            options,
            database,
            stack: Vec::new(),
            usage: PrefabExpansionUsage::default(),
            budget_failed: false,
            diagnostics: DiagnosticReport::default(),
            provenance: BTreeMap::new(),
            reserved_entity_ids: BTreeSet::new(),
            next_projected_entity: 0,
        }
    }

    fn expand_scene_document(&mut self, document: &SceneDocument) -> PrefabExpansionReport {
        let mut expanded = SceneDocument {
            roots: document.roots.clone(),
            entities: Vec::new(),
        };
        if self.reserve_provenance_bytes(
            Some(std::mem::size_of::<crate::SceneEntityProvenanceMap>()),
            None,
            None,
        ) && self
            .observe_item_budget(
                PrefabExpansionBudgetKind::MaterializedEntities,
                0,
                document.entities.len(),
                self.options.limits.materialized_entities(),
                None,
                None,
            )
            .is_some()
        {
            self.reserved_entity_ids
                .extend(document.entities.iter().map(|entity| entity.id.clone()));
            self.expand_entities(&document.entities, &mut expanded.entities);
        }
        expanded.canonicalize();

        if !self.diagnostics.has_errors() {
            let validation = self.validate_document(&expanded);
            self.diagnostics.extend(validation);
        }

        if self.diagnostics.has_errors() {
            return PrefabExpansionReport {
                candidate: None,
                diagnostics: std::mem::take(&mut self.diagnostics),
            };
        }

        let candidate =
            ExpandedSceneCandidate::from_expansion(expanded, std::mem::take(&mut self.provenance));
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
        }
    }

    fn expand_entities(
        &mut self,
        entities: &[SceneEntityRecord],
        output: &mut Vec<SceneEntityRecord>,
    ) {
        for entity in entities {
            if self.budget_failed {
                break;
            }
            self.expand_entity(entity, output);
        }
    }

    fn expand_entity(&mut self, entity: &SceneEntityRecord, output: &mut Vec<SceneEntityRecord>) {
        let Some(original_cost) = self.reserve_materialized_entity(entity) else {
            return;
        };
        if !self.reserve_provenance_bytes(
            provenance_entry_bytes(&entity.id, &entity.id),
            Some(&entity.id),
            entity.prefab.as_ref().map(|prefab| &prefab.source),
        ) {
            return;
        }
        let mut anchor = entity.clone();
        let Some(replacement_cost) =
            self.normalize_entity_components(&mut anchor, original_cost, &entity.id)
        else {
            return;
        };
        let Some((nodes, bytes)) = self.preflight_materialized_value_cost(
            original_cost.nodes(),
            replacement_cost.nodes(),
            original_cost.logical_bytes(),
            replacement_cost.logical_bytes(),
            &entity.id,
        ) else {
            return;
        };
        self.usage.materialized_value_nodes = nodes;
        self.usage.materialized_value_bytes = bytes;
        let prefab_instance = anchor.prefab.take();
        if let Some(instance) = prefab_instance {
            let prefab_roots = self.expand_prefab_instance(&entity.id, instance, output, None);
            anchor.children.splice(0..0, prefab_roots);
        }
        if self.budget_failed {
            return;
        }
        self.provenance.insert(
            anchor.id.clone(),
            SceneEntityProvenance::local(anchor.id.clone()),
        );
        output.push(anchor);
    }

    fn expand_prefab_instance(
        &mut self,
        anchor_id: &SceneEntityId,
        instance: PrefabInstance,
        output: &mut Vec<SceneEntityRecord>,
        parent_projection: Option<&PrefabProjectionPath>,
    ) -> Vec<SceneEntityId> {
        let PrefabInstance { source, overrides } = instance;
        if !self.reserve_resolved_instance(anchor_id, &source) {
            return Vec::new();
        }
        if self.stack.len() >= self.options.max_depth {
            self.diagnostics.push(with_asset_ref(
                with_public_u64(
                    with_public_u64(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.prefab-depth-exceeded",
                                    "Prefab expansion depth limit was exceeded",
                                ),
                                "entity-id",
                                anchor_id.as_str(),
                            ),
                            "field-path",
                            "prefab.source",
                        ),
                        "current-depth",
                        usize_to_u64(self.stack.len()),
                    ),
                    "maximum-depth",
                    usize_to_u64(self.options.max_depth),
                ),
                "asset-ref",
                &source,
            ));
            return Vec::new();
        }

        let resolved = self.resolver.resolve_prefab_binding(&source);
        let Some(resolved) = resolved else {
            self.diagnostics.push(with_asset_ref(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.prefab-source-missing",
                            "Prefab source could not be resolved",
                        ),
                        "entity-id",
                        anchor_id.as_str(),
                    ),
                    "field-path",
                    "prefab.source",
                ),
                "asset-ref",
                &source,
            ));
            return Vec::new();
        };
        let canonical_key = resolved.key();
        if canonical_key.is_some_and(|key| key.kind() != SourceDocumentKind::Prefab) {
            self.diagnostics.push(with_asset_ref(
                diagnostic_error(
                    "scene.prefab-source-kind-invalid",
                    "Resolved Prefab source identity does not describe a Prefab",
                ),
                "asset-ref",
                &source,
            ));
            return Vec::new();
        }
        if let Some((cycle_start_index, cycle_from)) = self
            .stack
            .iter()
            .position(|active_source| active_source.matches(&source, canonical_key))
            .zip(self.stack.last())
        {
            let diagnostic = with_public_u64(
                with_public_u64(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.prefab-cycle",
                                "Prefab source cycle was detected",
                            ),
                            "entity-id",
                            anchor_id.as_str(),
                        ),
                        "field-path",
                        "prefab.source",
                    ),
                    "cycle-start-index",
                    usize_to_u64(cycle_start_index),
                ),
                "cycle-depth",
                usize_to_u64(
                    self.stack
                        .len()
                        .saturating_sub(cycle_start_index)
                        .saturating_add(1),
                ),
            );
            let diagnostic = with_asset_ref(diagnostic, "cycle-from", &cycle_from.request);
            self.diagnostics
                .push(with_asset_ref(diagnostic, "cycle-to", &source));
            return Vec::new();
        }
        let prefab = resolved.document();

        if !self.reserve_applied_patch_operations(overrides.operations.len(), anchor_id, &source)
            || !self.preflight_source_materialization(prefab, &overrides, anchor_id, &source)
            || !self.reserve_provenance_bytes(
                ActivePrefabSource::allocation_bytes(&source, canonical_key),
                Some(anchor_id),
                Some(&source),
            )
        {
            return Vec::new();
        }

        let mut source_document = prefab.instantiate();
        let structure = validate_scene_structure(&source_document);
        if structure.has_errors() {
            self.diagnostics.extend(structure);
            return Vec::new();
        }

        self.stack.push(ActivePrefabSource {
            request: source.clone(),
            key: canonical_key.cloned(),
        });

        if !overrides.is_empty() {
            let patch_report = match self.database {
                Some(database) => overrides
                    .apply_owned_to_unpublished_scene_for_prefab_with_asset_database(
                        &mut source_document,
                        self.registry,
                        database,
                    ),
                None => overrides.apply_owned_to_unpublished_scene_for_prefab(
                    &mut source_document,
                    self.registry,
                ),
            };

            if !patch_report.applied {
                self.diagnostics.extend(patch_report.diagnostics);
                self.stack.pop();
                return Vec::new();
            }
        }

        let Some(entity_ids) = self.allocate_projected_entity_ids(&source_document, anchor_id)
        else {
            self.stack.pop();
            return Vec::new();
        };
        let roots = source_document.roots;
        let root_ids = roots.iter().collect::<BTreeSet<_>>();
        for entity in source_document.entities {
            if self.budget_failed {
                break;
            }
            let is_root = root_ids.contains(&entity.id);
            let provenance_bytes = PrefabProjectionPath::extended_allocation_bytes(
                parent_projection,
                anchor_id,
                &source,
                canonical_key,
                &entity.id,
            )
            .and_then(|bytes| {
                bytes.checked_add(provenance_entry_bytes(&entity_ids[&entity.id], &entity.id)?)
            });
            if !self.reserve_provenance_bytes(provenance_bytes, Some(anchor_id), Some(&source)) {
                break;
            }
            let projection = PrefabProjectionPath::from_resolved_step(
                parent_projection,
                anchor_id,
                &source,
                canonical_key,
                &entity.id,
            );
            self.expand_prefab_entity(anchor_id, entity, is_root, projection, &entity_ids, output);
        }

        self.stack.pop();
        if self.budget_failed {
            return Vec::new();
        }
        roots.iter().map(|root| entity_ids[root].clone()).collect()
    }

    fn allocate_projected_entity_ids(
        &mut self,
        document: &SceneDocument,
        anchor_id: &SceneEntityId,
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
                    self.fail_prefab_remap_budget(error, anchor_id);
                    return None;
                }
                self.next_projected_entity = ordinal
                    .checked_add(1)
                    .expect("in-memory entity allocation cannot exhaust usize ordinals");
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

    fn expand_prefab_entity(
        &mut self,
        anchor_id: &SceneEntityId,
        mut entity: SceneEntityRecord,
        is_root: bool,
        projection: PrefabProjectionPath,
        entity_ids: &BTreeMap<SceneEntityId, SceneEntityId>,
        output: &mut Vec<SceneEntityRecord>,
    ) {
        let Some(original_value_cost) = self.reserve_materialized_entity(&entity) else {
            return;
        };
        let source_entity_id = entity.id.clone();
        let prefab_instance = entity.prefab.take();
        let current_generated_bytes = self.usage.generated_identifier_bytes;
        let single_limit = self.options.limits.single_generated_identifier_bytes();
        let aggregate_limit = self.options.limits.generated_identifier_bytes();
        let materialized_value_byte_limit = self.options.limits.materialized_value_bytes().get();
        let traversal_limits = self.prefab_reference_traversal_limits();
        let mut generated_bytes = 0_usize;
        let mut original_reference_bytes = 0_usize;
        let mut mapped_reference_bytes = 0_usize;
        let mut reference_plans =
            Vec::<DeclaredEntityReferencePlan>::with_capacity(entity.components.len());

        // Entity IDs were charged when the complete source map was allocated.
        let identifier_lengths = entity
            .children
            .iter()
            .map(|child| entity_ids[child].as_str().len())
            .chain(is_root.then(|| entity_ids[&entity.id].as_str().len()));
        for identifier_len in identifier_lengths {
            if let Err(error) = accumulate_generated_identifier_bytes(
                current_generated_bytes,
                &mut generated_bytes,
                identifier_len,
                single_limit.get(),
                aggregate_limit.get(),
            ) {
                self.fail_prefab_remap_budget(error, anchor_id);
                return;
            }
        }

        let Some(replacement_value_cost) =
            self.normalize_entity_components(&mut entity, original_value_cost, anchor_id)
        else {
            return;
        };

        if self
            .preflight_materialized_value_cost(
                original_value_cost.nodes(),
                replacement_value_cost.nodes(),
                original_value_cost.logical_bytes(),
                replacement_value_cost.logical_bytes(),
                anchor_id,
            )
            .is_none()
        {
            return;
        }

        for (component_id, component) in &entity.components {
            let Some(schema) = self.registry.schema(component_id) else {
                self.diagnostics.push(with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.prefab-component-schema-missing",
                            "Prefab component schema was unavailable before publication",
                        ),
                        "entity-id",
                        entity.id.as_str(),
                    ),
                    "component-id",
                    component_id.as_str(),
                ));
                return;
            };
            let plan = match plan_declared_entity_references::<PrefabReferenceRemapError>(
                schema,
                &component.value,
                traversal_limits,
            ) {
                Ok(plan) => plan,
                Err(error) => {
                    self.push_prefab_reference_rewrite_error(
                        &entity.id,
                        component_id.as_str(),
                        &error,
                    );
                    return;
                }
            };
            if let Err(error) = plan.visit(&component.value, |_, reference| {
                if let EntityReference::SceneLocal { entity } = reference {
                    let mapped = entity_ids
                        .get(entity)
                        .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?;
                    let identifier_len = mapped.as_str().len();
                    accumulate_generated_identifier_bytes(
                        current_generated_bytes,
                        &mut generated_bytes,
                        identifier_len,
                        single_limit.get(),
                        aggregate_limit.get(),
                    )?;
                    original_reference_bytes = original_reference_bytes
                        .checked_add(entity.as_str().len())
                        .ok_or(PrefabReferenceRemapError::MaterializedValue {
                            observed: usize::MAX,
                            maximum: materialized_value_byte_limit,
                        })?;
                    mapped_reference_bytes = mapped_reference_bytes
                        .checked_add(identifier_len)
                        .ok_or(PrefabReferenceRemapError::MaterializedValue {
                            observed: usize::MAX,
                            maximum: materialized_value_byte_limit,
                        })?;
                }
                Ok(())
            }) {
                if let ComponentEntityReferenceRewriteError::Rewrite { error, .. } = &error
                    && self.fail_prefab_remap_budget(*error, anchor_id)
                {
                    return;
                }
                self.push_prefab_reference_rewrite_error(&entity.id, component_id.as_str(), &error);
                return;
            }
            reference_plans.push(plan);
        }

        let Some(replacement_value_bytes) = replacement_value_cost
            .logical_bytes()
            .checked_sub(original_reference_bytes)
            .and_then(|bytes| bytes.checked_add(mapped_reference_bytes))
        else {
            self.fail_budget(
                PrefabExpansionBudgetKind::MaterializedValueBytes,
                usize::MAX,
                materialized_value_byte_limit,
                Some(anchor_id),
                None,
            );
            return;
        };
        let Some((materialized_value_nodes, materialized_value_bytes)) = self
            .preflight_materialized_value_cost(
                original_value_cost.nodes(),
                replacement_value_cost.nodes(),
                original_value_cost.logical_bytes(),
                replacement_value_bytes,
                anchor_id,
            )
        else {
            return;
        };
        let Some(generated_identifier_bytes) = current_generated_bytes.checked_add(generated_bytes)
        else {
            self.fail_budget(
                PrefabExpansionBudgetKind::GeneratedIdentifierBytes,
                usize::MAX,
                aggregate_limit.get(),
                Some(anchor_id),
                None,
            );
            return;
        };

        for ((component_id, component), plan) in entity.components.iter_mut().zip(reference_plans) {
            if let Err(error) = plan.rewrite_in_place(&mut component.value, |_, reference| {
                Ok::<_, PrefabReferenceRemapError>(match reference {
                    EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                        entity: entity_ids
                            .get(entity)
                            .ok_or(PrefabReferenceRemapError::MissingSourceEntity)?
                            .clone(),
                    },
                    EntityReference::Persistent { .. } => reference.clone(),
                })
            }) {
                self.push_prefab_reference_rewrite_error(&entity.id, component_id.as_str(), &error);
                return;
            }
        }

        entity.children = entity
            .children
            .iter()
            .map(|child| entity_ids[child].clone())
            .collect();
        entity.id = entity_ids[&entity.id].clone();

        self.usage.generated_identifier_bytes = generated_identifier_bytes;
        self.usage.materialized_value_nodes = materialized_value_nodes;
        self.usage.materialized_value_bytes = materialized_value_bytes;

        if let Some(instance) = prefab_instance {
            let nested_anchor_id = entity.id.clone();
            let nested_roots =
                self.expand_prefab_instance(&nested_anchor_id, instance, output, Some(&projection));
            entity.children.splice(0..0, nested_roots);
        }
        if self.budget_failed {
            return;
        }
        self.provenance.insert(
            entity.id.clone(),
            SceneEntityProvenance::projected(source_entity_id, projection),
        );
        output.push(entity);
    }

    fn normalize_entity_components(
        &mut self,
        entity: &mut SceneEntityRecord,
        original_cost: ComponentValueCost,
        anchor_id: &SceneEntityId,
    ) -> Option<ComponentValueCost> {
        let limits = self.materialization_limits();
        let mut replaced_cost = ComponentValueCost::ZERO;
        let mut replacement_cost = ComponentValueCost::ZERO;
        for (component_id, component) in &mut entity.components {
            let value = std::mem::replace(&mut component.value, ComponentValue::Null);
            replaced_cost = replaced_cost.saturating_add(value.cost());
            let migrated = match self.registry.migrate_component_value_owned(
                component_id,
                component.version,
                value,
            ) {
                Ok(migrated) => migrated,
                Err(error) => {
                    self.diagnostics.push(with_migration_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.prefab-component-migration-failed",
                                    "Prefab component migration failed before publication",
                                ),
                                "entity-id",
                                entity.id.as_str(),
                            ),
                            "component-id",
                            component_id.as_str(),
                        ),
                        &error,
                    ));
                    return None;
                }
            };
            let Some(schema) = self.registry.schema(component_id) else {
                self.diagnostics.push(with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.prefab-component-schema-missing",
                            "Prefab component schema was unavailable before publication",
                        ),
                        "entity-id",
                        entity.id.as_str(),
                    ),
                    "component-id",
                    component_id.as_str(),
                ));
                return None;
            };
            let effective = match effective_component_value_owned(schema, migrated.value, limits) {
                Ok(effective) => effective,
                Err(error) => {
                    self.diagnostics.push(with_codec_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.prefab-default-materialization-failed",
                                    "Prefab component defaults exceeded their admitted interpretation",
                                ),
                                "entity-id", entity.id.as_str(),
                            ),
                            "component-id", component_id.as_str(),
                        ),
                        &error,
                        Some(schema),
                    ));
                    return None;
                }
            };
            component.version = migrated.version;
            component.value = effective;
            replacement_cost = replacement_cost.saturating_add(component.value.cost());
            let next_nodes = original_cost
                .nodes()
                .saturating_sub(replaced_cost.nodes())
                .saturating_add(replacement_cost.nodes());
            let next_bytes = original_cost
                .logical_bytes()
                .saturating_sub(replaced_cost.logical_bytes())
                .saturating_add(replacement_cost.logical_bytes());
            self.preflight_materialized_value_cost(
                original_cost.nodes(),
                next_nodes,
                original_cost.logical_bytes(),
                next_bytes,
                anchor_id,
            )?;
        }
        Some(replacement_cost)
    }

    fn reserve_materialized_entity(
        &mut self,
        entity: &SceneEntityRecord,
    ) -> Option<ComponentValueCost> {
        let value_cost = scene_entity_component_value_cost(entity);
        let entities = self.observe_item_budget(
            PrefabExpansionBudgetKind::MaterializedEntities,
            self.usage.materialized_entities,
            1,
            self.options.limits.materialized_entities(),
            Some(&entity.id),
            entity.prefab.as_ref().map(|prefab| &prefab.source),
        )?;
        let components = self.observe_item_budget(
            PrefabExpansionBudgetKind::MaterializedComponents,
            self.usage.materialized_components,
            entity.components.len(),
            self.options.limits.materialized_components(),
            Some(&entity.id),
            entity.prefab.as_ref().map(|prefab| &prefab.source),
        )?;
        let value_nodes = self.observe_item_budget(
            PrefabExpansionBudgetKind::MaterializedValueNodes,
            self.usage.materialized_value_nodes,
            value_cost.nodes(),
            self.options.limits.materialized_value_nodes(),
            Some(&entity.id),
            entity.prefab.as_ref().map(|prefab| &prefab.source),
        )?;
        let value_bytes = self.observe_byte_budget(
            PrefabExpansionBudgetKind::MaterializedValueBytes,
            self.usage.materialized_value_bytes,
            value_cost.logical_bytes(),
            self.options.limits.materialized_value_bytes(),
            Some(&entity.id),
            entity.prefab.as_ref().map(|prefab| &prefab.source),
        )?;
        self.usage.materialized_entities = entities;
        self.usage.materialized_components = components;
        self.usage.materialized_value_nodes = value_nodes;
        self.usage.materialized_value_bytes = value_bytes;
        Some(value_cost)
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
        self.usage.provenance_bytes = bytes;
        true
    }

    fn reserve_applied_patch_operations(
        &mut self,
        amount: usize,
        anchor_id: &SceneEntityId,
        source: &AssetRef,
    ) -> bool {
        let Some(operations) = self.observe_item_budget(
            PrefabExpansionBudgetKind::AppliedPatchOperations,
            self.usage.applied_patch_operations,
            amount,
            self.options.limits.applied_patch_operations(),
            Some(anchor_id),
            Some(source),
        ) else {
            return false;
        };
        self.usage.applied_patch_operations = operations;
        true
    }

    fn preflight_source_materialization(
        &mut self,
        prefab: &PrefabDocument,
        overrides: &ScenePatchDocument,
        anchor_id: &SceneEntityId,
        source: &AssetRef,
    ) -> bool {
        let value_cost = scene_entities_source_value_cost(&prefab.entities)
            .saturating_add(scene_patch_value_cost(overrides));
        let mut entities = prefab.entities.len();
        let mut components = prefab
            .entities
            .iter()
            .map(|entity| entity.components.len())
            .sum::<usize>();
        for operation in &overrides.operations {
            match operation {
                ScenePatchOperation::AddEntity { entity, .. } => {
                    entities = entities.saturating_add(1);
                    components = components.saturating_add(entity.components.len());
                }
                ScenePatchOperation::RestoreEntity {
                    entity,
                    descendants,
                    ..
                } => {
                    entities = entities.saturating_add(1_usize.saturating_add(descendants.len()));
                    components = components
                        .saturating_add(entity.components.len())
                        .saturating_add(
                            descendants
                                .iter()
                                .map(|descendant| descendant.components.len())
                                .sum::<usize>(),
                        );
                }
                ScenePatchOperation::AddComponent { .. } => {
                    components = components.saturating_add(1);
                }
                ScenePatchOperation::RemoveEntity { .. }
                | ScenePatchOperation::RemoveComponent { .. }
                | ScenePatchOperation::ReplaceComponent { .. }
                | ScenePatchOperation::SetField { .. }
                | ScenePatchOperation::RemoveField { .. }
                | ScenePatchOperation::MoveEntity { .. }
                | ScenePatchOperation::SetAssetRefField { .. } => {}
            }
        }

        self.observe_item_budget(
            PrefabExpansionBudgetKind::MaterializedEntities,
            self.usage.materialized_entities,
            entities,
            self.options.limits.materialized_entities(),
            Some(anchor_id),
            Some(source),
        )
        .is_some()
            && self
                .observe_item_budget(
                    PrefabExpansionBudgetKind::MaterializedComponents,
                    self.usage.materialized_components,
                    components,
                    self.options.limits.materialized_components(),
                    Some(anchor_id),
                    Some(source),
                )
                .is_some()
            && self
                .observe_item_budget(
                    PrefabExpansionBudgetKind::MaterializedValueNodes,
                    self.usage.materialized_value_nodes,
                    value_cost.nodes(),
                    self.options.limits.materialized_value_nodes(),
                    Some(anchor_id),
                    Some(source),
                )
                .is_some()
            && self
                .observe_byte_budget(
                    PrefabExpansionBudgetKind::MaterializedValueBytes,
                    self.usage.materialized_value_bytes,
                    value_cost.logical_bytes(),
                    self.options.limits.materialized_value_bytes(),
                    Some(anchor_id),
                    Some(source),
                )
                .is_some()
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
        anchor_id: &SceneEntityId,
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
        self.fail_budget(kind, observed, maximum, Some(anchor_id), None);
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

    fn validate_document(&self, document: &SceneDocument) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new()
            .with_materialization_limits(self.materialization_limits());
        if let Some(database) = self.database {
            context = context.with_project_asset_database(database);
        }
        preflight_scene_with_context(document, self.registry, &mut context).diagnostics
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
