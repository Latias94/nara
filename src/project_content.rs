//! Authorized, World-independent Scene content loading.

mod budget;
mod catalogue;
mod closure;
mod delivery;
mod domains;
mod scenes;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetMetaCandidate, AssetMetaFileLimits, AssetMetaIdentityCandidate, AssetPath, AssetPathScope,
    AssetProductRef, AssetRecord, AssetRef, AssetReferenceResolver, AssetSourceKind,
    AssetSourcePath, ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_core::ByteLimit;
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_fs::{ContentDigest, DirectoryCapability, FsError, FsOperation, RelativePath};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageImportError, ImageImportMemoryPlan,
    ImageImportSettings, ImageImporter, ImageImporterCreateError,
};
use nara_reflect::{
    __private::{
        AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
    },
    ComponentValueMaterializationLimits, DeclaredAssetReference, DeclaredAssetReferenceError,
    EntityReferenceTraversalLimits, SchemaCompositionFingerprint,
};
use nara_scene::{
    ExpandedSceneCandidate, PrefabDocument, PrefabDocumentCandidate, PrefabSourceResolver,
    ResolvedPrefabSource, SceneComponentBindings, SceneComponentBindingsWitness, SceneDocument,
    SceneDocumentCandidate, SceneEntityProvenanceMap, SceneEntityRecord, SceneFormatError,
    SourceDocumentKey, SourceDocumentKind,
};
use nara_tilemap::{
    TileSetSource, TileSetSourceFileLimits, TilemapMembershipError, tilemap_membership_work_bytes,
    validate_persisted_tilemap_membership,
};

use crate::project_diagnostic_ids::{fs_operation_id, io_error_kind_id};
use crate::project_host::{
    ProjectSettingsCandidate, ProjectSettingsLineage, RuntimePlan, SchemaValidationInput,
    runtime_agreement::PreparedRuntimeSchemaAgreement,
};
use crate::scene_retention::{
    ARC_CONTROL_BLOCK_BYTES, BTREE_ENTRY_OVERHEAD, STRING_ALLOCATION_OVERHEAD,
    VALUE_NODE_ALLOCATION_BYTES,
};
use crate::startup_scene::SceneSource;

use budget::{BudgetTicket, GENERATION_LEASE_RETAINED_BYTES};
pub use budget::{
    ProjectContentBudgetError, ProjectContentBudgetHost, ProjectContentBudgetKind,
    ProjectContentBudgetSnapshot, ProjectContentLimits,
};
pub(crate) use budget::{ProjectContentGenerationLease, ProjectContentLease};
pub(crate) use catalogue::DeliveryCatalogue;
use catalogue::ReferenceCatalogue;
pub(crate) use catalogue::asset_record_retained_bytes;
use closure::{ProjectContentClosureBuilder, ProjectContentClosureDraft};
pub use closure::{
    ProjectContentClosureReceipt, ProjectContentMemberKind, ProjectContentMemberReceipt,
    StartupContentClosureReceipt,
};
pub use delivery::ProjectContentDeliveryReceipt;
pub(crate) use domains::ProjectAssetDomains;
pub(crate) use domains::validate_asset_domain_kind;
#[cfg(feature = "tooling")]
pub(crate) use domains::{EditorSourceValidation, validate_editor_source};
pub(crate) use domains::{ErasedCollection, runtime_collection};
pub use domains::{ProjectAssetCollection, ProjectDomainAssets};
use scenes::pending_scene_source_work_bytes;

/// Computes the canonical Runtime startup-content digest from already admitted logical members.
///
/// The closure builder remains the sole owner of digest framing and ordering. Callers must
/// provide only members that belong to the Runtime content closure; manifest and schema-bundle
/// facts are intentionally excluded from this digest by the builder's invariant.
pub(crate) fn startup_content_digest_from_members(
    manifest_digest: ContentDigest,
    members: impl IntoIterator<
        Item = (
            ProjectContentMemberKind,
            nara_project::ProjectPath,
            ContentDigest,
        ),
    >,
) -> Result<ContentDigest, ProjectContentError> {
    let mut builder = ProjectContentClosureBuilder::new(manifest_digest)?;
    for (kind, path, digest) in members {
        builder.insert_project_path(kind, path, digest)?;
    }
    let framed_bytes = builder.content_digest_framed_bytes()?;
    builder.content_digest(framed_bytes)
}

// nara_fs relative traversal retains at most one intermediate handle while opening the next, and
// bounded reads clone the opened file for the reader. Reserve both slots before either operation.
const RELATIVE_OPEN_HANDLE_PEAK: usize = 2;
// Exact Linux traversal additionally holds an independent native directory cursor.
const EXACT_RELATIVE_OPEN_HANDLE_PEAK: usize = if cfg!(target_os = "linux") { 3 } else { 2 };
// Covers the Windows fixed normalized-name buffer and the smaller pinned Linux cursor buffer.
const EXACT_RELATIVE_OPEN_WORK_BYTES: usize = 65_536;
const RETAINED_DIRECTORY_HANDLES: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectContentRevision([u8; 32]);

impl ProjectContentRevision {
    #[must_use]
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(64);
        for byte in self.0 {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for ProjectContentRevision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProjectContentRevision")
            .field(&self.to_hex())
            .finish()
    }
}

#[derive(Debug)]
pub struct ProjectPrefabContent {
    path: AssetSourcePath,
    document: Arc<PrefabDocument>,
    source: SourceDocumentKey,
}

/// One captured authored Scene and its separately compiled effective data.
///
/// Dependency membership retains data only. It neither instantiates this Scene nor grants
/// hosted travel to it; the original project's declared Scene roots govern travel.
#[derive(Debug)]
pub struct ProjectSceneContent {
    path: AssetSourcePath,
    source: SourceDocumentKey,
    document: Arc<SceneDocument>,
    expanded: Arc<ExpandedSceneCandidate>,
}

impl ProjectSceneContent {
    #[must_use]
    pub const fn path(&self) -> &AssetSourcePath {
        &self.path
    }

    #[must_use]
    pub const fn source_key(&self) -> &SourceDocumentKey {
        &self.source
    }

    #[must_use]
    pub fn document(&self) -> &SceneDocument {
        &self.document
    }

    #[must_use]
    pub fn expanded_scene(&self) -> &SceneDocument {
        self.expanded.document()
    }
}

struct ProjectContentScenes {
    selected: ProjectSceneContent,
    dependencies: Box<[ProjectSceneContent]>,
}

impl ProjectPrefabContent {
    #[must_use]
    pub const fn path(&self) -> &AssetSourcePath {
        &self.path
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        &self.document
    }

    /// Returns the source identity admitted for this Prefab before expansion.
    #[must_use]
    pub const fn source_key(&self) -> &SourceDocumentKey {
        &self.source
    }
}

#[derive(Debug)]
pub struct ProjectImageContent {
    image: ImageAsset,
}

#[derive(Debug)]
pub struct ProjectTileSetContent {
    stable_id: nara_asset::StableAssetId,
    path: AssetSourcePath,
    image_path: AssetPath,
    metadata_digest: ContentDigest,
    source_hash: ContentDigest,
    source: TileSetSource,
}

impl ProjectTileSetContent {
    #[must_use]
    pub const fn stable_id(&self) -> nara_asset::StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetSourcePath {
        &self.path
    }

    #[must_use]
    pub const fn image_path(&self) -> &AssetPath {
        &self.image_path
    }

    #[must_use]
    pub const fn source_hash(&self) -> ContentDigest {
        self.source_hash
    }

    #[must_use]
    pub const fn source(&self) -> &TileSetSource {
        &self.source
    }
}

impl ProjectImageContent {
    #[must_use]
    pub fn path(&self) -> &AssetSourcePath {
        self.source().path()
    }

    /// Real source-import provenance captured by this source-only content owner.
    #[must_use]
    pub fn source(&self) -> &nara_image::ImageSourceMetadata {
        self.image
            .source()
            .expect("Project source content admits only imported Images")
    }

    #[must_use]
    pub const fn image(&self) -> &ImageAsset {
        &self.image
    }
}

/// One selected Scene and its exact expanded, decoded dependency closure.
///
/// Clones retain the same content generation. Source-only receipts obtained through
/// [`Self::content_closure`] retain their own fact budget without retaining decoded content.
/// Escaped Scene sources and shared Images retain the generation until their last owner drops.
#[derive(Clone)]
pub struct ProjectContentSnapshot {
    inner: Arc<ProjectContentSnapshotInner>,
}

struct ProjectContentSnapshotInner {
    schema_authority: Arc<SceneComponentBindingsWitness>,
    domain_authority: ProjectAssetDomains,
    content_closure: ProjectContentClosureReceipt,
    source_upgrade_required: bool,
    prefabs: Box<[ProjectPrefabContent]>,
    images: Box<[ProjectImageContent]>,
    tilesets: Box<[ProjectTileSetContent]>,
    domain_assets: ProjectDomainAssets,
    sources: Arc<ProjectSourceContent>,
    _generation_lease: ProjectContentGenerationLease,
    _runtime_lease: ProjectContentLease,
}

// Pixel consumers retain the byte reservation and generation claim without retaining the
// snapshot, its Images, or any decoded Scene candidate back through this owner.
struct ProjectImageRetention {
    _lease: ProjectContentLease,
    _generation: ProjectContentGenerationLease,
}

/// Immutable source data and reference context retained together by escaped materializers.
pub(crate) struct ProjectSourceContent {
    database: ProjectAssetDatabase,
    asset_scope: AssetPathScope,
    prefab_scope: AssetPathScope,
    // Escaped materializers retain the actual flat Scene data, not only its accounting lease.
    scenes: ProjectContentScenes,
    _lease: ProjectContentLease,
}

impl ProjectSourceContent {
    pub(crate) fn database(&self) -> &ProjectAssetDatabase {
        &self.database
    }

    pub(crate) fn resolution(&self) -> AssetReferenceResolver<'_> {
        self.database.resolver(&self.asset_scope)
    }
}

impl ProjectContentSnapshot {
    pub(crate) fn matches_asset_domains(&self, plan: &RuntimePlan) -> bool {
        self.inner.domain_authority.matches(&plan.asset_domains)
    }

    #[must_use]
    pub fn lineage(&self) -> ProjectSettingsLineage {
        self.inner.content_closure.settings_lineage()
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.inner.content_closure.schema_fingerprint()
    }

    #[must_use]
    pub(crate) fn shares_component_bindings(&self, bindings: &SceneComponentBindings) -> bool {
        self.inner.schema_authority.matches(bindings)
    }

    #[must_use]
    pub(crate) fn has_runtime_schema_agreement(&self) -> bool {
        self.inner.content_closure.has_runtime_schema_agreement()
    }

    #[must_use]
    pub fn revision(&self) -> ProjectContentRevision {
        self.inner.content_closure.content_revision()
    }

    #[must_use]
    pub fn content_digest(&self) -> ContentDigest {
        self.inner.content_closure.content_digest()
    }

    /// Returns independently retained source facts, without retaining this decoded generation.
    #[must_use]
    pub fn content_closure(&self) -> ProjectContentClosureReceipt {
        self.inner.content_closure.clone()
    }

    /// Returns startup attestation only when this snapshot selected the manifest's default Scene.
    #[must_use]
    pub fn startup_content_closure(&self) -> Option<StartupContentClosureReceipt> {
        self.inner.content_closure.startup_receipt()
    }

    /// The selected path, relative to the configured Scenes directory.
    #[must_use]
    pub fn selected_scene_path(&self) -> &nara_project::ProjectPath {
        self.inner.content_closure.selected_scene_path()
    }

    /// Returns the canonical source identity admitted for the selected Scene document.
    #[must_use]
    pub fn selected_scene_source_key(&self) -> &SourceDocumentKey {
        &self.inner.sources.scenes.selected.source
    }

    /// Returns the actual primary source Product admitted for the selected Scene, when known.
    #[must_use]
    pub fn selected_scene_product(&self) -> Option<AssetProductRef> {
        self.inner
            .sources
            .database
            .record_for_path(&self.inner.sources.scenes.selected.path)
            .and_then(|record| {
                AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY).ok()
            })
    }

    /// Flat effective Scene data dependencies, excluding the selected root.
    #[must_use]
    pub fn scene_dependencies(&self) -> &[ProjectSceneContent] {
        &self.inner.sources.scenes.dependencies
    }

    #[must_use]
    pub fn source_upgrade_required(&self) -> bool {
        self.inner.source_upgrade_required
    }

    /// Returns the captured source document before effective selection and component migration.
    /// Use [`Self::expanded_scene`] for the admitted runtime data.
    #[must_use]
    pub fn scene(&self) -> &SceneDocument {
        self.inner.sources.scenes.selected.document()
    }

    /// Returns selected, expanded and migrated Scene data admitted for runtime preparation.
    #[must_use]
    pub fn expanded_scene(&self) -> &SceneDocument {
        self.inner.sources.scenes.selected.expanded_scene()
    }

    /// Returns the structured source identity paired with the expanded selected document.
    #[must_use]
    pub fn expanded_scene_provenance(&self) -> &SceneEntityProvenanceMap {
        self.inner.sources.scenes.selected.expanded.provenance()
    }

    #[must_use]
    pub fn prefabs(&self) -> &[ProjectPrefabContent] {
        &self.inner.prefabs
    }

    #[must_use]
    pub fn images(&self) -> &[ProjectImageContent] {
        &self.inner.images
    }

    #[must_use]
    pub fn tilesets(&self) -> &[ProjectTileSetContent] {
        &self.inner.tilesets
    }

    #[must_use]
    pub fn domain_assets(&self) -> &ProjectDomainAssets {
        &self.inner.domain_assets
    }

    pub(crate) fn asset_database(&self) -> &ProjectAssetDatabase {
        self.inner.sources.database()
    }

    /// Borrows the actual catalogue and authored asset-path scope admitted for this content.
    #[must_use]
    pub fn asset_resolution(&self) -> AssetReferenceResolver<'_> {
        self.inner.sources.resolution()
    }

    /// Borrows the admitted declaration scope for Prefab source aliases.
    #[must_use]
    pub fn prefab_resolution(&self) -> AssetReferenceResolver<'_> {
        self.inner
            .sources
            .database()
            .resolver(&self.inner.sources.prefab_scope)
    }

    pub(crate) fn expanded_candidate(&self) -> &nara_scene::ExpandedSceneCandidate {
        &self.inner.sources.scenes.selected.expanded
    }

    pub(crate) fn reserve_consumer_retained(
        &self,
        bytes: usize,
    ) -> Result<ProjectContentLease, ProjectContentError> {
        self.inner
            ._runtime_lease
            .reserve_retained(bytes)
            .map_err(ProjectContentError::budget)
    }

    pub(crate) fn share_image_for_consumer(&self, index: usize) -> Option<ImageAsset> {
        self.inner
            .images
            .get(index)
            .and_then(|content| content.image.share_retained())
    }

    /// Retains the exact expanded selected source for one runtime materialization.
    #[must_use]
    pub fn scene_source(&self) -> SceneSource {
        SceneSource::from_project_content(
            Arc::clone(&self.inner.sources.scenes.selected.expanded),
            Arc::clone(&self.inner.sources),
            self.inner._runtime_lease.clone(),
            self.inner._generation_lease.clone(),
            startup_scene_retained_bytes(&self.inner.sources.scenes.selected.expanded)
                .expect("an admitted Scene already has a valid retained-byte plan"),
            Arc::clone(&self.inner.schema_authority),
        )
    }

    pub(crate) fn prepare_editor_startup_scene(
        &self,
        source_key: &SourceDocumentKey,
        document: &SceneDocument,
        bindings: &SceneComponentBindings,
    ) -> Result<SceneSource, ProjectContentError> {
        if !self.shares_component_bindings(bindings) {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::SchemaMismatch,
                "project.content.schema-mismatch",
                "Editor startup Scene bindings do not match the admitted content",
            ));
        }
        let resolver = SnapshotPrefabResolver {
            prefabs: self.prefabs(),
            database: self.inner.sources.database(),
            scope: &self.inner.sources.prefab_scope,
        };
        let expansion = match bindings.try_expand_prefabs_with_loader(
            document,
            nara_scene::PrefabExpansionOptions::default(),
            Some(self.asset_resolution()),
            |source, _| Ok::<_, std::convert::Infallible>(resolver.resolve_prefab(source)),
        ) {
            Ok(report) => report,
            Err(error) => match error {},
        };
        let nara_scene::PrefabExpansionReport {
            candidate,
            diagnostics,
            ..
        } = expansion;
        let expanded_candidate = candidate
            .ok_or_else(|| {
                map_scene_diagnostics(ProjectContentErrorKind::PrefabExpansion, diagnostics)
            })?
            .bind_source(source_key.clone());
        let expanded = expanded_candidate.document();
        let diagnostics = bindings.validate_scene(
            expanded,
            Some(self.asset_resolution()),
            ComponentValueMaterializationLimits::default(),
        );
        if diagnostics.has_errors() {
            return Err(map_scene_diagnostics(
                ProjectContentErrorKind::ScenePublication,
                diagnostics,
            ));
        }
        self.validate_editor_product_selections(expanded, bindings)?;
        let retained_bytes = editor_startup_scene_retained_bytes(&expanded_candidate)?;
        let lease = self
            .inner
            ._runtime_lease
            .reserve_retained(retained_bytes)
            .map_err(ProjectContentError::budget)?;
        Ok(SceneSource::from_project_content(
            Arc::new(expanded_candidate),
            Arc::clone(&self.inner.sources),
            lease,
            self.inner._generation_lease.clone(),
            retained_bytes,
            Arc::clone(&self.inner.schema_authority),
        ))
    }

    fn validate_editor_product_selections(
        &self,
        document: &SceneDocument,
        bindings: &SceneComponentBindings,
    ) -> Result<(), ProjectContentError> {
        let mut budget = self.inner._runtime_lease.budget_ticket();
        let mut required = RequiredAssetProducts::default();
        for entity in &document.entities {
            for (component, value) in &entity.components {
                let schema = bindings.schema(component).ok_or_else(|| {
                    ProjectContentError::single(
                        ProjectContentErrorKind::AssetReference,
                        "project.content.asset-reference-invalid",
                        "Editor Scene component has no admitted asset-reference schema",
                    )
                })?;
                let (references, work_bytes) =
                    collect_effective_asset_references(schema, value, &mut budget)?;
                for reference in references {
                    budget
                        .add(ProjectContentBudgetKind::DependencyEdges, 1)
                        .map_err(ProjectContentError::budget)?;
                    let resolution = self.asset_resolution();
                    let resolution_work =
                        resolution
                            .resolution_work_bytes(reference.asset_ref())
                            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
                    budget
                        .add(ProjectContentBudgetKind::WorkBytes, resolution_work)
                        .map_err(ProjectContentError::budget)?;
                    let source = resolution.resolve_ref(reference.asset_ref()).map_err(|_| {
                        ProjectContentError::single(
                            ProjectContentErrorKind::AssetReference,
                            "project.content.asset-reference-unresolved",
                            "Editor Scene asset source is outside the admitted content",
                        )
                    })?;
                    budget.subtract(ProjectContentBudgetKind::WorkBytes, resolution_work);
                    if self.inner.domain_authority.supports(source.source_kind()) {
                        require_asset_product(
                            &mut required,
                            &mut budget,
                            required_product(reference.asset_ref(), source)?,
                        )?;
                    } else if !reference.asset_ref().is_primary() {
                        return Err(unsupported_product_selection(reference.asset_ref()));
                    }
                }
                budget.subtract(ProjectContentBudgetKind::WorkBytes, work_bytes);
            }
        }
        self.inner
            .domain_assets
            .validate_product_selections(&mut required)
    }

    #[cfg(feature = "presentation-2d")]
    pub(crate) fn sync_editor_preview_scene(
        &self,
        projection: &mut nara_scene::SceneProjection,
        source_key: &SourceDocumentKey,
        document: &SceneDocument,
        world: &mut nara_ecs::World,
        registry: &nara_reflect::ComponentRegistry,
    ) -> nara_scene::SceneSpawnReport {
        let resolver = SnapshotPrefabResolver {
            prefabs: self.prefabs(),
            database: self.inner.sources.database(),
            scope: &self.inner.sources.prefab_scope,
        };
        let expansion = document.expand_prefabs_with_options_and_asset_resolution(
            registry,
            &resolver,
            nara_scene::PrefabExpansionOptions::default(),
            self.asset_resolution(),
        );
        let Some(candidate) = expansion.candidate else {
            let mut report = nara_scene::SceneSpawnReport::default();
            report.diagnostics = expansion.diagnostics;
            return report;
        };
        projection.publish(
            world,
            registry,
            &candidate.bind_source(source_key.clone()),
            self.asset_resolution(),
        )
    }
}

struct SnapshotPrefabResolver<'a> {
    prefabs: &'a [ProjectPrefabContent],
    database: &'a ProjectAssetDatabase,
    scope: &'a AssetPathScope,
}

impl PrefabSourceResolver for SnapshotPrefabResolver<'_> {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<ResolvedPrefabSource> {
        if !source.is_primary() {
            return None;
        }
        self.prefabs
            .iter()
            .find(|prefab| match source {
                AssetRef::Path(path) => {
                    self.scope.relative_path(prefab.path()) == Some(path.as_str())
                }
                AssetRef::StableId(id) => {
                    self.database
                        .record_for_stable_id(*id)
                        .is_some_and(|record| {
                            record.source_kind() == &AssetSourceKind::Prefab
                                && record.path() == prefab.path()
                        })
                }
                AssetRef::Product(product) => self
                    .database
                    .record_for_stable_id(product.source())
                    .is_some_and(|record| {
                        record.source_kind() == &AssetSourceKind::Prefab
                            && record.path() == prefab.path()
                    }),
            })
            .and_then(|prefab| {
                let source = ResolvedPrefabSource::bound(
                    Arc::clone(&prefab.document),
                    prefab.source.clone(),
                );
                match self.database.record_for_path(prefab.path()) {
                    Some(record) => source.with_source_asset(record).ok(),
                    None => Some(source),
                }
            })
    }
}

impl fmt::Debug for ProjectContentSnapshot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectContentSnapshot")
            .field("lineage", &self.lineage())
            .field("schema_fingerprint", &self.schema_fingerprint())
            .field("revision", &self.revision())
            .field("content_digest", &self.content_digest())
            .field("selected_scene_path", self.selected_scene_path())
            .field("prefab_count", &self.inner.prefabs.len())
            .field("image_count", &self.inner.images.len())
            .field("tileset_count", &self.inner.tilesets.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectContentErrorKind {
    InvalidLimits,
    ProjectLineageMismatch,
    ProjectRootUnbound,
    ProjectRootMismatch,
    MissingStartupScene,
    MissingSelectedScene,
    InvalidLogicalPath,
    ClosurePathCollision,
    HostIo,
    HostAuthorityUnsupported,
    HostAuthorityUnproven,
    HostAuthorityRejected,
    BudgetExceeded,
    SceneFormat,
    ScenePublication,
    PrefabExpansion,
    AssetReference,
    AssetMeta,
    AssetMetaMismatch,
    UnsupportedAssetKind,
    AssetDomain,
    ImageImport,
    AllocationFailed,
    UnsupportedHierarchySemantics,
    SchemaMismatch,
    GenerationCapacityExceeded,
}

#[derive(Clone, PartialEq)]
pub struct ProjectContentError {
    kind: ProjectContentErrorKind,
    diagnostics: Box<DiagnosticReport>,
}

impl ProjectContentError {
    /// Preserves a domain-authored diagnostic without reinterpreting its format or settings.
    #[must_use]
    pub(crate) fn asset_domain(diagnostic: Diagnostic) -> Self {
        let mut report = DiagnosticReport::default();
        report.push(diagnostic);
        Self::with_report(ProjectContentErrorKind::AssetDomain, report)
    }

    #[must_use]
    pub const fn kind(&self) -> ProjectContentErrorKind {
        self.kind
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        self.diagnostics.as_ref()
    }

    pub(crate) fn runtime_agreement(code: &'static str, summary: &'static str) -> Self {
        Self::single(ProjectContentErrorKind::SchemaMismatch, code, summary)
    }

    pub(crate) fn runtime_agreement_with_identifier(
        code: &'static str,
        summary: &'static str,
        key: &'static str,
        value: &str,
    ) -> Self {
        Self::with_identifier(
            ProjectContentErrorKind::SchemaMismatch,
            code,
            summary,
            key,
            value,
        )
    }

    pub(crate) fn runtime_agreement_host(error: FsError) -> Self {
        map_fs_error(error)
    }

    fn single(kind: ProjectContentErrorKind, code: &'static str, summary: &'static str) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic(code, summary));
        Self::with_report(kind, diagnostics)
    }

    fn with_report(kind: ProjectContentErrorKind, diagnostics: DiagnosticReport) -> Self {
        Self {
            kind,
            diagnostics: Box::new(diagnostics),
        }
    }

    fn with_identifier(
        kind: ProjectContentErrorKind,
        code: &'static str,
        summary: &'static str,
        key: &'static str,
        value: &str,
    ) -> Self {
        let diagnostic = PublicDiagnosticIdentifier::new(value).map_or_else(
            |_| with_sensitive(diagnostic(code, summary), key),
            |value| {
                attach_field(
                    diagnostic(code, summary),
                    DiagnosticField::public_identifier(field_key(key), value),
                )
            },
        );
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        Self::with_report(kind, diagnostics)
    }

    fn budget(error: ProjectContentBudgetError) -> Self {
        let diagnostic = attach_field(
            attach_field(
                attach_field(
                    attach_field(
                        diagnostic(
                            "project.content.budget-exceeded",
                            "Project content budget was exceeded",
                        ),
                        DiagnosticField::public_identifier(
                            field_key("budget"),
                            PublicDiagnosticIdentifier::new(error.kind().as_str())
                                .expect("budget identifiers are engine-owned"),
                        ),
                    ),
                    DiagnosticField::public_u64(
                        field_key("requested"),
                        u64::try_from(error.requested()).unwrap_or(u64::MAX),
                    ),
                ),
                DiagnosticField::public_u64(
                    field_key("active"),
                    u64::try_from(error.active()).unwrap_or(u64::MAX),
                ),
            ),
            DiagnosticField::public_u64(
                field_key("limit"),
                u64::try_from(error.limit()).unwrap_or(u64::MAX),
            ),
        );
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        Self::with_report(ProjectContentErrorKind::BudgetExceeded, diagnostics)
    }
}

impl fmt::Debug for ProjectContentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectContentError")
            .field("kind", &self.kind)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl Display for ProjectContentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            ProjectContentErrorKind::InvalidLimits => "project content limits are invalid",
            ProjectContentErrorKind::ProjectLineageMismatch => {
                "project content inputs have different lineages"
            }
            ProjectContentErrorKind::ProjectRootUnbound => {
                "project settings were not ingested from a project root"
            }
            ProjectContentErrorKind::ProjectRootMismatch => {
                "project content root does not match project settings"
            }
            ProjectContentErrorKind::MissingStartupScene => "project startup Scene is missing",
            ProjectContentErrorKind::MissingSelectedScene => "selected project Scene is missing",
            ProjectContentErrorKind::InvalidLogicalPath => {
                "project content contains an invalid logical path"
            }
            ProjectContentErrorKind::ClosurePathCollision => {
                "project Scene closure contains conflicting logical path facts"
            }
            ProjectContentErrorKind::HostIo => "project content host I/O failed",
            ProjectContentErrorKind::HostAuthorityUnsupported => {
                "project content host authority is unsupported"
            }
            ProjectContentErrorKind::HostAuthorityUnproven => {
                "project content host authority is unproven"
            }
            ProjectContentErrorKind::HostAuthorityRejected => {
                "project content host authority was rejected"
            }
            ProjectContentErrorKind::BudgetExceeded => "project content budget was exceeded",
            ProjectContentErrorKind::SceneFormat => "project scene format is invalid",
            ProjectContentErrorKind::ScenePublication => {
                "project scene semantic publication failed"
            }
            ProjectContentErrorKind::PrefabExpansion => "project prefab expansion failed",
            ProjectContentErrorKind::AssetReference => {
                "project component asset reference is invalid"
            }
            ProjectContentErrorKind::AssetMeta => "project asset metadata is invalid",
            ProjectContentErrorKind::AssetMetaMismatch => {
                "project asset metadata does not match its source"
            }
            ProjectContentErrorKind::UnsupportedAssetKind => {
                "project Scene content references an unsupported asset kind"
            }
            ProjectContentErrorKind::AssetDomain => "project asset domain preparation failed",
            ProjectContentErrorKind::ImageImport => "project image import failed",
            ProjectContentErrorKind::AllocationFailed => {
                "project content allocation failed after budget admission"
            }
            ProjectContentErrorKind::UnsupportedHierarchySemantics => {
                "project content requires unsupported hierarchy semantics"
            }
            ProjectContentErrorKind::SchemaMismatch => {
                "project schema validation authority changed"
            }
            ProjectContentErrorKind::GenerationCapacityExceeded => {
                "project content generation capacity was exceeded"
            }
        })
    }
}

impl Error for ProjectContentError {}

pub struct ProjectContentLoader {
    root: DirectoryCapability,
    limits: ProjectContentLimits,
    budget_host: ProjectContentBudgetHost,
    image_importer: ImageImporter,
}

impl ProjectContentLoader {
    pub fn new(root: DirectoryCapability) -> Result<Self, ProjectContentError> {
        Self::with_limits(root, ProjectContentLimits::default())
    }

    pub fn with_limits(
        root: DirectoryCapability,
        limits: ProjectContentLimits,
    ) -> Result<Self, ProjectContentError> {
        let budget_host = ProjectContentBudgetHost::new(limits);
        Self::with_budget_host(root, limits, budget_host)
    }

    pub fn with_budget_host(
        root: DirectoryCapability,
        limits: ProjectContentLimits,
        budget_host: ProjectContentBudgetHost,
    ) -> Result<Self, ProjectContentError> {
        if budget_host.limits() != limits {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::InvalidLimits,
                "project.content.budget-host-mismatch",
                "Project content budget host limits do not match loader limits",
            ));
        }
        let image_importer = ImageImporter::with_limits(limits.image_import())
            .map_err(map_image_importer_create_error)?;
        Ok(Self {
            root,
            limits,
            budget_host,
            image_importer,
        })
    }

    #[must_use]
    pub fn budget_snapshot(&self) -> ProjectContentBudgetSnapshot {
        self.budget_host.snapshot()
    }

    #[must_use]
    pub fn budget_host(&self) -> ProjectContentBudgetHost {
        self.budget_host.clone()
    }

    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.root.share()
    }

    /// Loads the default startup Scene and its required dependency closure.
    pub fn load(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(candidate, plan, None, None, BTreeMap::new(), None)
    }

    /// Loads one Scene path relative to the configured Scenes directory.
    ///
    /// This source-backed operation preserves the admitted project settings and schema authority.
    /// It neither changes the default startup Scene nor declares content for delivery.
    pub fn load_scene(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        scene: &nara_project::ProjectPath,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(
            candidate,
            plan,
            Some(SourceSceneSelection::Path(scene)),
            None,
            BTreeMap::new(),
            None,
        )
    }

    /// Resolves a primary Scene Product and its declared travel root in source preparation.
    pub(crate) fn load_scene_product(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        product: AssetProductRef,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(
            candidate,
            plan,
            Some(SourceSceneSelection::Product(product)),
            None,
            BTreeMap::new(),
            None,
        )
    }

    pub(crate) fn load_with_runtime_agreement(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        agreement: PreparedRuntimeSchemaAgreement,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(
            candidate,
            plan,
            None,
            None,
            BTreeMap::new(),
            Some(agreement),
        )
    }

    pub(crate) fn load_with_expected_startup_scene_digest_and_runtime_agreement(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        expected_startup_scene_digest: ContentDigest,
        agreement: PreparedRuntimeSchemaAgreement,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(
            candidate,
            plan,
            None,
            Some(expected_startup_scene_digest),
            BTreeMap::new(),
            Some(agreement),
        )
    }

    pub(crate) fn load_with_expected_prefab_digests_and_runtime_agreement<'source>(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        expected_sources: impl IntoIterator<Item = (&'source AssetPath, ContentDigest)>,
        agreement: PreparedRuntimeSchemaAgreement,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        let mut expectation_budget = self.budget_host.reserve();
        let mut expected_prefabs = BTreeMap::new();
        for (path, digest) in expected_sources {
            let root = &candidate.settings().paths.prefabs;
            let work = checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    size_of::<(AssetSourcePath, ContentDigest)>(),
                    BTREE_ENTRY_OVERHEAD,
                    root.as_str().len(),
                    1,
                    path.as_str()
                        .len()
                        .checked_add(STRING_ALLOCATION_OVERHEAD)
                        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
                ],
            )?;
            expectation_budget
                .add(ProjectContentBudgetKind::WorkBytes, work)
                .map_err(ProjectContentError::budget)?;
            let canonical = AssetSourcePath::new(format!("{}/{}", root.as_str(), path.as_str()))
                .map_err(|_| catalogue::identity_conflict(path))?;
            expected_prefabs.insert(canonical, digest);
        }
        self.load_inner(
            candidate,
            plan,
            None,
            None,
            expected_prefabs,
            Some(agreement),
        )
    }

    fn load_inner(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        selected_scene: Option<SourceSceneSelection<'_>>,
        expected_startup_scene_digest: Option<ContentDigest>,
        expected_prefabs: BTreeMap<AssetSourcePath, ContentDigest>,
        runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner_with_delivery_catalogue(
            candidate,
            plan,
            selected_scene,
            expected_startup_scene_digest,
            expected_prefabs,
            runtime_agreement,
            None,
            false,
        )
        .map(|(snapshot, _)| snapshot)
    }

    pub(crate) fn load_startup_for_delivery(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        expected_startup_scene_digest: Option<ContentDigest>,
        agreement: PreparedRuntimeSchemaAgreement,
    ) -> Result<(ProjectContentSnapshot, Option<DeliveryCatalogue>), ProjectContentError> {
        self.load_inner_with_delivery_catalogue(
            candidate,
            plan,
            None,
            expected_startup_scene_digest,
            BTreeMap::new(),
            Some(agreement),
            None,
            true,
        )
    }

    fn load_scene_for_delivery(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        scene: &nara_project::ProjectPath,
        catalogue: Option<&DeliveryCatalogue>,
        capture_catalogue: bool,
    ) -> Result<(ProjectContentSnapshot, Option<DeliveryCatalogue>), ProjectContentError> {
        self.load_inner_with_delivery_catalogue(
            candidate,
            plan,
            Some(SourceSceneSelection::Path(scene)),
            None,
            BTreeMap::new(),
            None,
            catalogue,
            capture_catalogue,
        )
    }

    fn load_inner_with_delivery_catalogue(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        selected_scene: Option<SourceSceneSelection<'_>>,
        expected_startup_scene_digest: Option<ContentDigest>,
        expected_prefabs: BTreeMap<AssetSourcePath, ContentDigest>,
        runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
        delivery_catalogue: Option<&DeliveryCatalogue>,
        capture_delivery_catalogue: bool,
    ) -> Result<(ProjectContentSnapshot, Option<DeliveryCatalogue>), ProjectContentError> {
        self.validate_inputs(candidate, plan)?;
        let settings = candidate.settings();
        if delivery_catalogue.is_some_and(|catalogue| {
            catalogue.roots
                != [
                    settings.paths.assets.clone(),
                    settings.paths.prefabs.clone(),
                    settings.paths.scenes.clone(),
                ]
        }) {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.delivery.catalogue-authority-mismatch",
                "Delivery catalogue does not match the admitted asset roots",
            ));
        }
        if runtime_agreement.as_ref().is_some_and(|agreement| {
            !agreement.matches_inputs(self.root.identity(), candidate, plan)
        }) {
            return Err(ProjectContentError::runtime_agreement(
                "project.runtime-agreement.input-mismatch",
                "Prepared runtime schema agreement does not match the content load inputs",
            ));
        }
        let validation = plan.schema_validation();
        let mut context = LoadContext::new(
            self,
            validation,
            &plan.asset_domains,
            expected_startup_scene_digest,
            expected_prefabs,
            candidate.manifest_digest(),
            runtime_agreement,
            delivery_catalogue,
            capture_delivery_catalogue,
        )?;
        let snapshot = context.load(candidate, selected_scene)?;
        Ok((snapshot, context.take_delivery_catalogue()))
    }

    fn validate_inputs(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
    ) -> Result<(), ProjectContentError> {
        if candidate.lineage() != plan.lineage() {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::ProjectLineageMismatch,
                "project.content.lineage-mismatch",
                "Project content inputs have different settings lineages",
            ));
        }
        let Some(root_identity) = candidate.lineage().project_root_identity() else {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::ProjectRootUnbound,
                "project.content.root-unbound",
                "Project settings were not ingested from a project root capability",
            ));
        };
        if root_identity != self.root.identity() {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::ProjectRootMismatch,
                "project.content.root-mismatch",
                "Project content root does not match the settings authority",
            ));
        }
        let validation = plan.schema_validation();
        let bindings = validation.bindings();
        if !validation.registry().shares_snapshot(validation.snapshot())
            || !validation.snapshot().ptr_eq(bindings.native_snapshot())
            || bindings.composition_fingerprint() != validation.composition_fingerprint()
        {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::SchemaMismatch,
                "project.content.schema-mismatch",
                "Project schema validation fingerprint changed",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for ProjectContentLoader {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectContentLoader")
            .field("root_identity", &self.root.identity())
            .field("limits", &self.limits)
            .field("budget", &self.budget_snapshot())
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct SourceCandidates {
    images: BTreeSet<AssetPath>,
    tilesets: BTreeSet<AssetPath>,
    domains: BTreeSet<AssetPath>,
    scenes: BTreeMap<AssetSourcePath, PendingSceneSource>,
    seen_scenes: BTreeSet<AssetSourcePath>,
    scene_index_work_bytes: usize,
    products: RequiredAssetProducts,
    product_work_bytes: usize,
}

type RequiredAssetProducts = BTreeMap<StableAssetId, BTreeSet<AssetProductRef>>;

struct PendingSceneSource {
    reader: usize,
    alias: AssetPath,
}

struct NativeSourceSelection {
    path: AssetSourcePath,
    reader: usize,
    alias: AssetPath,
}

#[derive(Clone, Copy)]
enum SourceSceneSelection<'a> {
    Path(&'a nara_project::ProjectPath),
    Product(AssetProductRef),
}

struct LoadContext<'a> {
    loader: &'a ProjectContentLoader,
    validation: &'a SchemaValidationInput,
    domains: &'a ProjectAssetDomains,
    budget: Option<BudgetTicket>,
    source_upgrade_required: bool,
    expected_startup_scene_digest: Option<ContentDigest>,
    expected_prefabs: BTreeMap<AssetSourcePath, ContentDigest>,
    runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
    content_closure: Option<ProjectContentClosureBuilder>,
    content_closure_work_bytes: usize,
    native_source_work_bytes: usize,
    asset_database: ProjectAssetDatabase,
    asset_scope: AssetPathScope,
    prefab_scope: AssetPathScope,
    scene_scope: AssetPathScope,
    path_scope_work_bytes: usize,
    asset_database_work_bytes: usize,
    catalogue: Option<ReferenceCatalogue>,
    catalogue_roots: Option<Vec<nara_project::ProjectPath>>,
    capture_delivery_catalogue: bool,
    completed_delivery_catalogue: Option<DeliveryCatalogue>,
}

struct LoadedPrefabContent {
    document: Arc<PrefabDocument>,
    source: SourceDocumentKey,
    path: AssetSourcePath,
}

fn resolved_prefab_source(
    loaded: &LoadedPrefabContent,
    database: &ProjectAssetDatabase,
) -> Result<ResolvedPrefabSource, ProjectContentError> {
    let source = ResolvedPrefabSource::bound(Arc::clone(&loaded.document), loaded.source.clone());
    match database.record_for_path(&loaded.path) {
        Some(record) => source.with_source_asset(record).map_err(|_| {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.prefab-source-association-invalid",
                "Prefab metadata conflicts with its admitted document source",
                "prefab",
                loaded.path.as_str(),
            )
        }),
        None => Ok(source),
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkReservation {
    reserved: usize,
}

fn source_scope(root: &nara_project::ProjectPath) -> Result<AssetPathScope, ProjectContentError> {
    AssetSourcePath::new(root.as_str())
        .map(AssetPathScope::under)
        .map_err(|_| {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::InvalidLogicalPath,
                "project.content.source-path-invalid",
                "Configured source directory has an invalid logical location",
                "source",
                root.as_str(),
            )
        })
}

impl<'a> LoadContext<'a> {
    fn asset_resolution(&self) -> AssetReferenceResolver<'_> {
        self.asset_database.resolver(&self.asset_scope)
    }

    fn catalogue_scope(&self, index: usize) -> &AssetPathScope {
        let root = &self
            .catalogue_roots
            .as_ref()
            .expect("catalogue readers are bound before discovery")[index];
        self.scope_for_root(root)
            .expect("each catalogue reader retains its actual source scope")
    }

    fn scope_for_root(&self, root: &nara_project::ProjectPath) -> Option<&AssetPathScope> {
        [&self.asset_scope, &self.prefab_scope, &self.scene_scope]
            .into_iter()
            .find(|scope| {
                scope
                    .prefix()
                    .is_some_and(|prefix| prefix.as_str() == root.as_str())
            })
    }

    fn new(
        loader: &'a ProjectContentLoader,
        validation: &'a SchemaValidationInput,
        domains: &'a ProjectAssetDomains,
        expected_startup_scene_digest: Option<ContentDigest>,
        expected_prefabs: BTreeMap<AssetSourcePath, ContentDigest>,
        manifest_digest: ContentDigest,
        runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
        delivery_catalogue: Option<&DeliveryCatalogue>,
        capture_delivery_catalogue: bool,
    ) -> Result<Self, ProjectContentError> {
        let content_closure_work_bytes = content_closure_entry_work_bytes("nara.toml".len())?;
        let catalogue_work_bytes =
            delivery_catalogue.map_or(0, |catalogue| catalogue.catalogue.work_bytes);
        let asset_database_work_bytes =
            delivery_catalogue.map_or(0, |catalogue| catalogue.asset_database_work_bytes);
        let mut budget = loader.budget_host.reserve();
        budget
            .add(
                ProjectContentBudgetKind::WorkBytes,
                content_closure_work_bytes
                    .checked_add(catalogue_work_bytes)
                    .and_then(|bytes| bytes.checked_add(asset_database_work_bytes))
                    .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
            )
            .map_err(ProjectContentError::budget)?;
        budget
            .add(
                ProjectContentBudgetKind::RetainedBytes,
                component_bindings_witness_retained_bytes(validation.bindings())?,
            )
            .map_err(ProjectContentError::budget)?;
        Ok(Self {
            loader,
            validation,
            domains,
            budget: Some(budget),
            source_upgrade_required: false,
            expected_startup_scene_digest,
            expected_prefabs,
            runtime_agreement,
            content_closure: Some(ProjectContentClosureBuilder::new(manifest_digest)?),
            content_closure_work_bytes,
            native_source_work_bytes: 0,
            asset_database: delivery_catalogue
                .map_or_else(ProjectAssetDatabase::default, |catalogue| {
                    catalogue.asset_database.clone()
                }),
            asset_database_work_bytes,
            asset_scope: AssetPathScope::ROOT,
            prefab_scope: AssetPathScope::ROOT,
            scene_scope: AssetPathScope::ROOT,
            path_scope_work_bytes: 0,
            catalogue: delivery_catalogue.map(|catalogue| catalogue.catalogue.clone()),
            catalogue_roots: None,
            capture_delivery_catalogue,
            completed_delivery_catalogue: None,
        })
    }

    fn load(
        &mut self,
        candidate: &ProjectSettingsCandidate,
        selected_scene: Option<SourceSceneSelection<'_>>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.add_budget(ProjectContentBudgetKind::OpenHandles, 1)?;
        let settings = candidate.settings();
        let scope_work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                string_retained_bytes(settings.paths.assets.as_str())?,
                string_retained_bytes(settings.paths.prefabs.as_str())?,
                string_retained_bytes(settings.paths.scenes.as_str())?,
                3 * size_of::<nara_project::ProjectPath>(),
                string_retained_bytes(settings.paths.assets.as_str())?,
                string_retained_bytes(settings.paths.prefabs.as_str())?,
                string_retained_bytes(settings.paths.scenes.as_str())?,
            ],
        )?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, scope_work)?;
        self.path_scope_work_bytes = scope_work;
        self.asset_scope = source_scope(&settings.paths.assets)?;
        self.prefab_scope = source_scope(&settings.paths.prefabs)?;
        self.scene_scope = source_scope(&settings.paths.scenes)?;
        self.catalogue_roots = Some(vec![
            settings.paths.assets.clone(),
            settings.paths.prefabs.clone(),
            settings.paths.scenes.clone(),
        ]);
        self.content_closure
            .as_mut()
            .expect("load context owns one Scene closure builder until publication")
            .set_runtime_directories([
                &settings.paths.assets,
                &settings.paths.scenes,
                &settings.paths.prefabs,
            ]);
        let scenes = self.open_directory(&settings.paths.scenes)?;
        let prefabs = self.open_directory(&settings.paths.prefabs)?;
        let assets = self.open_directory(&settings.paths.assets)?;
        let mut selected_product_path = None;
        let mut selected_product_path_work = None;
        let scene_path = match selected_scene {
            Some(SourceSceneSelection::Path(path)) => Some(path),
            Some(SourceSceneSelection::Product(product)) => {
                let reference = AssetRef::Product(product);
                let (resolved_source, work) = self.resolve_native_source(
                    &reference,
                    &AssetSourceKind::Scene,
                    &settings.paths.scenes,
                )?;
                let local = self
                    .scene_scope
                    .relative_path(&resolved_source.path)
                    .ok_or_else(|| {
                        ProjectContentError::single(
                            ProjectContentErrorKind::HostAuthorityRejected,
                            "project.content.scene-not-declared",
                            "Scene Product is outside the admitted travel Scene directory",
                        )
                    })?;
                let selected_work = self.reserve_work(string_retained_bytes(local)?)?;
                let local = self
                    .scene_scope
                    .relative_path(&resolved_source.path)
                    .expect("the travel source relation was checked before admission");
                selected_product_path = Some(
                    nara_project::ProjectPath::new(local)
                        .map_err(|_| catalogue::identity_conflict(&resolved_source.path))?,
                );
                selected_product_path_work = Some(selected_work);
                drop(resolved_source);
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, work.reserved);
                let resolved = selected_product_path
                    .as_ref()
                    .expect("source Product resolved above");
                if settings.startup.default_scene.as_ref() != Some(resolved)
                    && !settings.content.scenes.contains(resolved)
                {
                    return Err(ProjectContentError::single(
                        ProjectContentErrorKind::HostAuthorityRejected,
                        "project.content.scene-not-declared",
                        "Scene Product does not select startup or a declared delivery Scene",
                    ));
                }
                Some(resolved)
            }
            None => settings.startup.default_scene.as_ref(),
        }
        .ok_or_else(|| {
            ProjectContentError::single(
                ProjectContentErrorKind::MissingStartupScene,
                "project.content.startup-scene-missing",
                "Project manifest does not select a startup scene",
            )
        })?;
        let is_startup = settings.startup.default_scene.as_ref() == Some(scene_path);
        let selected_path_work = string_retained_bytes(scene_path.as_str())?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, selected_path_work)?;
        self.content_closure_work_bytes = self
            .content_closure_work_bytes
            .checked_add(selected_path_work)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;

        let (scene_bytes, scene_digest) = self.read_selected_scene_file(
            &scenes,
            scene_path.as_str(),
            self.loader.limits.scene_file().encoded_bytes().get(),
            is_startup,
        )?;
        if self
            .expected_startup_scene_digest
            .is_some_and(|expected| scene_digest != expected)
        {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.startup-scene-source-changed",
                "Project startup Scene bytes do not match the prepared source digest",
            ));
        }
        self.record_member(
            ProjectContentMemberKind::Scene,
            &settings.paths.scenes,
            scene_path.as_str(),
            scene_digest,
        )?;
        let selected_scene_source = self.admit_native_source(
            &scenes,
            &settings.paths.scenes,
            scene_path.as_str(),
            SourceDocumentKind::Scene,
            settings.project.native_document_identity,
            settings.project.source_universe,
        )?;
        let selected_path_work = self.reserve_work(checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                settings.paths.scenes.as_str().len(),
                1,
                string_retained_bytes(scene_path.as_str())?,
            ],
        )?)?;
        let selected_source_path = AssetSourcePath::new(format!(
            "{}/{}",
            settings.paths.scenes.as_str(),
            scene_path.as_str()
        ))
        .map_err(|_| catalogue::identity_conflict(scene_path))?;
        self.commit_work_as_retained(selected_path_work, selected_path_work.reserved)?;
        let scene = self.decode_scene(scene_path.as_str(), &scene_bytes)?;

        let mut loaded_prefabs = BTreeMap::<AssetSourcePath, LoadedPrefabContent>::new();
        let mut prefab_cache_work = 0usize;
        let expanded_candidate = self.compile_scene(
            &scene,
            &selected_scene_source,
            settings,
            &mut loaded_prefabs,
            &mut prefab_cache_work,
        )?;
        let validation = self.validation;
        let bindings = validation.bindings();
        let expanded = expanded_candidate.document();
        let provenance = expanded_candidate.provenance();

        if let Some(agreement) = self.runtime_agreement.take() {
            let (path, digest) = agreement.revalidate_and_into_closure_member(&self.loader.root)?;
            self.record_project_member(
                ProjectContentMemberKind::AuthoringSchemaBundle,
                path,
                digest,
            )?;
        }

        let mut sources = SourceCandidates::default();
        let root_index_work = checked_plan_sum(
            ProjectContentBudgetKind::WorkBytes,
            [
                size_of::<AssetSourcePath>(),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(selected_source_path.as_str())?,
            ],
        )?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, root_index_work)?;
        sources.scene_index_work_bytes = root_index_work;
        sources.seen_scenes.insert(selected_source_path.clone());
        self.collect_source_candidates(expanded, &assets, &mut sources)?;
        let mut dependent_scenes = Vec::new();
        while let Some((canonical, request)) = sources.scenes.pop_first() {
            self.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            let request_work = pending_scene_source_work_bytes(&canonical, &request.alias)?;
            let entry_work =
                self.reserve_work(size_of::<ProjectSceneContent>() + 3 * ARC_CONTROL_BLOCK_BYTES)?;
            dependent_scenes
                .try_reserve_exact(1)
                .map_err(|_| allocation_failed("project.content.scene-dependencies-allocation"))?;
            let root_work = self.reserve_work(string_retained_bytes(
                self.catalogue_roots
                    .as_ref()
                    .expect("source roots are bound")[request.reader]
                    .as_str(),
            )?)?;
            let source_root = self
                .catalogue_roots
                .as_ref()
                .expect("source roots are bound")[request.reader]
                .clone();
            let directory = self.open_directory(&source_root)?;
            let captured = self.capture_scene_dependency(
                &directory,
                &source_root,
                settings,
                &request.alias,
                &mut loaded_prefabs,
                &mut prefab_cache_work,
            )?;
            drop((directory, source_root));
            self.subtract_budget(ProjectContentBudgetKind::OpenHandles, 1);
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, root_work.reserved);
            self.collect_source_candidates(captured.expanded_scene(), &assets, &mut sources)?;
            dependent_scenes.push(captured);
            self.commit_work_as_retained(entry_work, entry_work.reserved)?;
            drop((canonical, request));
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, request_work);
        }
        drop(sources.seen_scenes);
        self.subtract_budget(
            ProjectContentBudgetKind::WorkBytes,
            sources.scene_index_work_bytes,
        );
        if let Some(path) = self.expected_prefabs.keys().next() {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.saved-prefab-outside-closure",
                "Saved Prefab is no longer a member of the selected Scene content closure",
                "prefab",
                path.as_str(),
            ));
        }
        let domain_collections = self.load_domains(
            &assets,
            &settings.paths.assets,
            sources.domains,
            &mut sources.products,
        )?;
        drop(sources.products);
        self.subtract_budget(
            ProjectContentBudgetKind::WorkBytes,
            sources.product_work_bytes,
        );
        let (tilesets, tileset_work) =
            self.load_tilesets(&assets, &settings.paths.assets, sources.tilesets)?;
        let mut image_paths = sources.images;
        for tileset in &tilesets {
            if !image_paths.contains(&tileset.image_path) {
                self.add_budget(
                    ProjectContentBudgetKind::WorkBytes,
                    queued_source_path_work_bytes(&tileset.image_path)?,
                )?;
                self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
                image_paths.insert(tileset.image_path.clone());
            }
        }
        let images = self.load_images(&assets, &settings.paths.assets, image_paths)?;
        // Asset dependencies can complete discovery after Prefab expansion. Reconcile every
        // catalogued Prefab only after all discovery, before identity/closure publication.
        if settings.project.native_document_identity
            == nara_project::NativeDocumentIdentity::ProjectLocation
        {
            for loaded in loaded_prefabs.values() {
                self.reconcile_native_metadata(&loaded.path, &loaded.source)?;
            }
        }
        self.admit_catalogued_native_metadata(
            &scenes,
            &settings.paths.scenes,
            scene_path.as_str(),
            &selected_source_path,
            &selected_scene_source,
        )?;
        for dependency in &dependent_scenes {
            self.reconcile_native_metadata(&dependency.path, &dependency.source)?;
        }
        self.validate_tileset_image_extents(&tilesets, &images)?;
        self.validate_tilemap_membership(&expanded, &provenance, &tilesets)?;
        self.commit_tileset_identities(&settings.paths.assets, &tilesets)?;
        let diagnostics = bindings.validate_scene(
            expanded,
            Some(self.asset_resolution()),
            ComponentValueMaterializationLimits::default(),
        );
        if diagnostics.has_errors() {
            return Err(map_scene_diagnostics(
                ProjectContentErrorKind::ScenePublication,
                diagnostics,
            ));
        }
        for dependency in &dependent_scenes {
            self.validate_tilemap_membership(
                dependency.expanded.document(),
                dependency.expanded.provenance(),
                &tilesets,
            )?;
            let diagnostics = bindings.validate_scene(
                dependency.expanded_scene(),
                Some(self.asset_resolution()),
                ComponentValueMaterializationLimits::default(),
            );
            if diagnostics.has_errors() {
                return Err(map_scene_diagnostics(
                    ProjectContentErrorKind::ScenePublication,
                    diagnostics,
                ));
            }
        }
        self.commit_work_as_retained(tileset_work, tileset_work.reserved)?;
        let database = self.retain_loaded_asset_identities(
            &selected_source_path,
            &dependent_scenes,
            &loaded_prefabs,
            &images,
            &tilesets,
            &domain_collections,
        )?;
        self.release_catalogue_work();
        drop((scenes, prefabs, assets));
        let open_handles = self.budget_value(ProjectContentBudgetKind::OpenHandles);
        self.subtract_budget(ProjectContentBudgetKind::OpenHandles, open_handles);

        let content_digest = self.finish_content_digest()?;
        let revision = content_revision(
            candidate.lineage(),
            self.validation.composition_fingerprint(),
            content_digest,
            scene_path,
        )?;
        let content_closure = self
            .content_closure
            .take()
            .expect("load context owns one Scene closure builder until publication")
            .finish(
                scene_path.clone(),
                is_startup,
                candidate.lineage(),
                self.validation.composition_fingerprint(),
                revision,
                content_digest,
            );
        drop(selected_product_path);
        if let Some(work) = selected_product_path_work {
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, work.reserved);
        }

        let snapshot_overhead = snapshot_retained_overhead(
            &selected_scene_source,
            &dependent_scenes,
            &loaded_prefabs,
            images.as_slice(),
            tilesets.as_slice(),
        )?;
        let content_closure_retained = content_closure_retained_bytes(&content_closure)?;
        let mut snapshot_work = WorkReservation {
            reserved: self.native_source_work_bytes,
        };
        self.resize_work(&mut snapshot_work, snapshot_overhead)?;
        self.commit_work_as_retained(snapshot_work, snapshot_overhead)?;
        self.native_source_work_bytes = 0;

        let selected_scene = ProjectSceneContent {
            path: selected_source_path,
            source: selected_scene_source,
            document: Arc::new(scene),
            expanded: Arc::new(expanded_candidate),
        };
        let mut prefab_values = Vec::new();
        prefab_values
            .try_reserve_exact(loaded_prefabs.len())
            .map_err(|_| allocation_failed("project.content.prefab-list-allocation-failed"))?;
        prefab_values.extend(
            loaded_prefabs
                .into_values()
                .map(|loaded| ProjectPrefabContent {
                    path: loaded.path,
                    document: loaded.document,
                    source: loaded.source,
                }),
        );
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, prefab_cache_work);
        let prefabs = prefab_values.into_boxed_slice();
        let mut images = images.into_boxed_slice();
        let tilesets = tilesets.into_boxed_slice();

        let mut content_closure_budget = self.loader.budget_host.reserve();
        content_closure_budget
            .set(
                ProjectContentBudgetKind::RetainedBytes,
                content_closure_retained,
            )
            .map_err(ProjectContentError::budget)?;
        let content_closure_lease = content_closure_budget
            .into_lease()
            .map_err(ProjectContentError::budget)?;
        self.subtract_budget(
            ProjectContentBudgetKind::WorkBytes,
            self.content_closure_work_bytes,
        );
        self.content_closure_work_bytes = 0;
        let retained_scopes = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                string_retained_bytes(settings.paths.assets.as_str())?,
                string_retained_bytes(settings.paths.prefabs.as_str())?,
            ],
        )?;
        drop(self.catalogue_roots.take());
        drop(std::mem::replace(
            &mut self.scene_scope,
            AssetPathScope::ROOT,
        ));
        let scope_reservation = WorkReservation {
            reserved: self.path_scope_work_bytes,
        };
        self.commit_work_as_retained(scope_reservation, retained_scopes)?;
        self.path_scope_work_bytes = 0;
        let budget = self
            .budget
            .take()
            .expect("load context owns one budget ticket until publication");
        let runtime_lease = budget.into_lease().map_err(ProjectContentError::budget)?;
        let generation_lease = self
            .loader
            .budget_host
            .retain_generation(revision)
            .map_err(|error| {
                let diagnostic = attach_field(
                    attach_field(
                        diagnostic(
                            "project.content.generation-capacity-exceeded",
                            "Project content retains too many distinct generations; retire an older Runtime or snapshot before retrying",
                        ),
                        DiagnosticField::public_u64(
                            field_key("active_generations"),
                            u64::try_from(error.active()).unwrap_or(u64::MAX),
                        ),
                    ),
                    DiagnosticField::public_u64(
                        field_key("generation_limit"),
                        u64::try_from(error.limit()).unwrap_or(u64::MAX),
                    ),
                );
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(diagnostic);
                ProjectContentError::with_report(
                    ProjectContentErrorKind::GenerationCapacityExceeded,
                    diagnostics,
                )
            })?;
        let sources = Arc::new(ProjectSourceContent {
            database,
            asset_scope: std::mem::replace(&mut self.asset_scope, AssetPathScope::ROOT),
            prefab_scope: std::mem::replace(&mut self.prefab_scope, AssetPathScope::ROOT),
            scenes: ProjectContentScenes {
                selected: selected_scene,
                dependencies: dependent_scenes.into_boxed_slice(),
            },
            _lease: runtime_lease.clone(),
        });
        if !images.is_empty() {
            let image_retention = Arc::new(ProjectImageRetention {
                _lease: runtime_lease.clone(),
                _generation: generation_lease.clone(),
            });
            for content in &mut images {
                if !content
                    .image
                    .try_attach_retention_owner(Arc::clone(&image_retention))
                {
                    return Err(ProjectContentError::single(
                        ProjectContentErrorKind::ImageImport,
                        "project.content.image-retention-conflict",
                        "Project image retention owner was already installed",
                    ));
                }
            }
        }
        let content_closure =
            content_closure.publish(self.loader.root.share(), content_closure_lease);

        Ok(ProjectContentSnapshot {
            inner: Arc::new(ProjectContentSnapshotInner {
                schema_authority: Arc::new(bindings.witness()),
                domain_authority: self.domains.clone(),
                content_closure,
                source_upgrade_required: self.source_upgrade_required,
                prefabs,
                images,
                tilesets,
                domain_assets: ProjectDomainAssets::new(
                    domain_collections,
                    runtime_lease.clone(),
                    generation_lease.clone(),
                ),
                sources,
                _generation_lease: generation_lease,
                _runtime_lease: runtime_lease,
            }),
        })
    }

    fn open_directory(
        &mut self,
        path: &nara_project::ProjectPath,
    ) -> Result<DirectoryCapability, ProjectContentError> {
        let relative = self.observe_path(path.as_str())?;
        self.add_budget(
            ProjectContentBudgetKind::OpenHandles,
            EXACT_RELATIVE_OPEN_HANDLE_PEAK,
        )?;
        let name_work = self.reserve_work(EXACT_RELATIVE_OPEN_WORK_BYTES)?;
        let opened = self.loader.root.open_directory_exact(&relative);
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, name_work.reserved);
        match opened {
            Ok(directory) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::OpenHandles,
                    EXACT_RELATIVE_OPEN_HANDLE_PEAK - RETAINED_DIRECTORY_HANDLES,
                );
                Ok(directory)
            }
            Err(error) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::OpenHandles,
                    EXACT_RELATIVE_OPEN_HANDLE_PEAK,
                );
                Err(map_fs_error(error))
            }
        }
    }

    fn read_file(
        &mut self,
        directory: &DirectoryCapability,
        logical_path: &str,
        encoded_ceiling: usize,
    ) -> Result<(Vec<u8>, ContentDigest), ProjectContentError> {
        self.read_file_inner(directory, logical_path, encoded_ceiling, None, false)
    }

    fn read_native_file(
        &mut self,
        directory: &DirectoryCapability,
        logical_path: &str,
        encoded_ceiling: usize,
    ) -> Result<(Vec<u8>, ContentDigest), ProjectContentError> {
        self.read_file_inner(directory, logical_path, encoded_ceiling, None, true)
    }

    fn read_selected_scene_file(
        &mut self,
        directory: &DirectoryCapability,
        logical_path: &str,
        encoded_ceiling: usize,
        is_startup: bool,
    ) -> Result<(Vec<u8>, ContentDigest), ProjectContentError> {
        self.read_file_inner(
            directory,
            logical_path,
            encoded_ceiling,
            Some(is_startup),
            true,
        )
    }

    fn read_file_inner(
        &mut self,
        directory: &DirectoryCapability,
        logical_path: &str,
        encoded_ceiling: usize,
        selected_scene: Option<bool>,
        exact_spelling: bool,
    ) -> Result<(Vec<u8>, ContentDigest), ProjectContentError> {
        let open_handle_peak = if exact_spelling {
            EXACT_RELATIVE_OPEN_HANDLE_PEAK
        } else {
            RELATIVE_OPEN_HANDLE_PEAK
        };
        let relative = self.observe_path(logical_path)?;
        self.add_budget(ProjectContentBudgetKind::Files, 1)?;
        self.add_budget(ProjectContentBudgetKind::OpenHandles, open_handle_peak)?;
        let name_work = self.reserve_work(if exact_spelling {
            EXACT_RELATIVE_OPEN_WORK_BYTES
        } else {
            0
        })?;
        let opened = if exact_spelling {
            directory.open_file_exact(&relative)
        } else {
            directory.open_file(&relative)
        };
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, name_work.reserved);
        let file = match opened {
            Ok(file) => file,
            Err(error) => {
                self.subtract_budget(ProjectContentBudgetKind::OpenHandles, open_handle_peak);
                if let Some(is_startup) = selected_scene
                    && fs_error_is_not_found(&error)
                {
                    return Err(ProjectContentError::with_identifier(
                        if is_startup {
                            ProjectContentErrorKind::MissingStartupScene
                        } else {
                            ProjectContentErrorKind::MissingSelectedScene
                        },
                        if is_startup {
                            "project.content.selected-startup-scene-missing"
                        } else {
                            "project.content.selected-scene-missing"
                        },
                        if is_startup {
                            "Selected project startup Scene is missing"
                        } else {
                            "Selected project Scene is missing"
                        },
                        "scene",
                        logical_path,
                    ));
                }
                return Err(map_fs_error(error));
            }
        };
        let encoded_before = self.budget_value(ProjectContentBudgetKind::EncodedBytes);
        self.set_budget(
            ProjectContentBudgetKind::EncodedBytes,
            encoded_before
                .checked_add(encoded_ceiling)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::EncodedBytes))?,
        )?;
        let result = file.read_to_end_bounded(u64::try_from(encoded_ceiling).unwrap_or(u64::MAX));
        drop(file);
        self.subtract_budget(ProjectContentBudgetKind::OpenHandles, open_handle_peak);
        match result {
            Ok(bytes) => {
                self.set_budget(
                    ProjectContentBudgetKind::EncodedBytes,
                    encoded_before
                        .checked_add(bytes.len())
                        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::EncodedBytes))?,
                )?;
                let digest = ContentDigest::of_bytes(&bytes);
                Ok((bytes, digest))
            }
            Err(error) => {
                self.set_budget(ProjectContentBudgetKind::EncodedBytes, encoded_before)
                    .expect("restoring an encoded reservation cannot exceed its limit");
                Err(map_fs_error(error))
            }
        }
    }

    fn load_prefab(
        &mut self,
        prefabs: &DirectoryCapability,
        prefab_root: &nara_project::ProjectPath,
        path: &AssetPath,
        canonical: &AssetSourcePath,
        identity_policy: nara_project::NativeDocumentIdentity,
        source_universe: nara_core::SourceUniverseId,
    ) -> Result<LoadedPrefabContent, ProjectContentError> {
        let (bytes, digest) = self.read_native_file(
            prefabs,
            path.as_str(),
            self.loader.limits.scene_file().encoded_bytes().get(),
        )?;
        let expected = self.expected_prefabs.remove(canonical);
        if let Some(expected) = expected
            && expected != digest
        {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.saved-prefab-source-changed",
                "Saved Prefab bytes do not match the accepted persistence receipt",
                "prefab",
                path.as_str(),
            ));
        }
        self.record_member(
            ProjectContentMemberKind::Prefab,
            prefab_root,
            path.as_str(),
            digest,
        )?;
        let source = self.admit_native_source(
            prefabs,
            prefab_root,
            path.as_str(),
            SourceDocumentKind::Prefab,
            identity_policy,
            source_universe,
        )?;
        Ok(LoadedPrefabContent {
            document: Arc::new(self.decode_prefab(path.as_str(), &bytes)?),
            source,
            path: canonical.clone(),
        })
    }

    fn admit_native_source(
        &mut self,
        directory: &DirectoryCapability,
        source_root: &nara_project::ProjectPath,
        logical_path: &str,
        kind: SourceDocumentKind,
        identity_policy: nara_project::NativeDocumentIdentity,
        source_universe: nara_core::SourceUniverseId,
    ) -> Result<SourceDocumentKey, ProjectContentError> {
        let project_path_bytes = source_root
            .as_str()
            .len()
            .checked_add(1)
            .and_then(|length| length.checked_add(logical_path.len()))
            .and_then(|length| length.checked_add(STRING_ALLOCATION_OVERHEAD))
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let key_bytes = size_of::<SourceDocumentKey>()
            .checked_add(match identity_policy {
                nara_project::NativeDocumentIdentity::ProjectLocation => project_path_bytes,
                nara_project::NativeDocumentIdentity::StableMetadata => 0,
            })
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let key_work = self.reserve_work(key_bytes)?;
        let temporary_work = if identity_policy
            == nara_project::NativeDocumentIdentity::StableMetadata
        {
            Some(
                self.reserve_work(checked_plan_sum(
                    ProjectContentBudgetKind::WorkBytes,
                    [
                        project_path_bytes,
                        string_retained_bytes(logical_path)?,
                        string_retained_bytes(logical_path)?
                            .checked_add(".meta".len())
                            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
                    ],
                )?)?,
            )
        } else {
            None
        };
        let project_path =
            AssetSourcePath::new(format!("{}/{}", source_root.as_str(), logical_path)).map_err(
                |_| {
                    ProjectContentError::with_identifier(
                        ProjectContentErrorKind::InvalidLogicalPath,
                        "project.content.source-path-invalid",
                        "Native document source path is invalid",
                        "source",
                        logical_path,
                    )
                },
            )?;
        let key = match identity_policy {
            nara_project::NativeDocumentIdentity::ProjectLocation => {
                SourceDocumentKey::project_location(source_universe, project_path, kind)
            }
            nara_project::NativeDocumentIdentity::StableMetadata => {
                let relative_path = AssetPath::new(logical_path).map_err(|_| {
                    ProjectContentError::single(
                        ProjectContentErrorKind::InvalidLogicalPath,
                        "project.content.source-path-invalid",
                        "Native document source path is invalid",
                    )
                })?;
                let metadata_path = format!("{logical_path}.meta");
                let ceiling = AssetMetaFileLimits::default().encoded_bytes().get();
                let (metadata_bytes, metadata_digest) =
                    self.read_native_file(directory, &metadata_path, ceiling)?;
                self.verify_catalogued_metadata(&project_path, source_root, metadata_digest)?;
                self.record_member(
                    ProjectContentMemberKind::AssetMetadata,
                    source_root,
                    &metadata_path,
                    metadata_digest,
                )?;
                let reservation = self.reserve_work(ceiling)?;
                let identity = AssetMetaIdentityCandidate::decode_json_bytes_owned(
                    metadata_bytes.into_boxed_slice(),
                )
                .map_err(|_| {
                    ProjectContentError::with_identifier(
                        ProjectContentErrorKind::AssetMeta,
                        "project.content.native-source-meta-invalid",
                        "Stable native document metadata is invalid",
                        "source",
                        logical_path,
                    )
                })?
                .into_identity();
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reservation.reserved);
                let expected_kind = match kind {
                    SourceDocumentKind::Scene => AssetSourceKind::Scene,
                    SourceDocumentKind::Prefab => AssetSourceKind::Prefab,
                };
                if identity.source_kind() != &expected_kind {
                    return Err(ProjectContentError::with_identifier(
                        ProjectContentErrorKind::AssetMetaMismatch,
                        "project.content.native-source-meta-mismatch",
                        "Stable native document metadata does not match its source",
                        "source",
                        logical_path,
                    ));
                }
                let key = SourceDocumentKey::stable_asset(identity.stable_id(), kind);
                self.catalogue
                    .get_or_insert_with(ReferenceCatalogue::default);
                let root_index = self
                    .catalogue_roots
                    .as_ref()
                    .and_then(|roots| roots.iter().position(|root| root == source_root))
                    .expect("native source reader belongs to the configured catalogue roots");
                // The read capability supplies this alias; the sidecar can be bound through
                // another overlapping configured scope to the same canonical source. The
                // catalogue validates that complete relation before the key can escape.
                self.index_metadata_identity(relative_path, root_index, metadata_digest, identity)?;
                drop((project_path, metadata_path));
                self.subtract_budget(
                    ProjectContentBudgetKind::WorkBytes,
                    temporary_work
                        .expect("stable source admission owns its path scratch")
                        .reserved,
                );
                key
            }
        };
        self.native_source_work_bytes = self
            .native_source_work_bytes
            .checked_add(key_work.reserved)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        Ok(key)
    }

    fn decode_scene(
        &mut self,
        path: &str,
        bytes: &[u8],
    ) -> Result<SceneDocument, ProjectContentError> {
        let work_before =
            self.reserve_work(scene_decode_work_plan(self.loader.limits.scene_file())?)?;
        let document = decode_scene_candidate(path, bytes, self.loader.limits.scene_file())?
            .into_composition_source()
            .map_err(map_scene_publication_error)?;
        let retained = scene_retained_bytes(&document)?;
        self.commit_work_as_retained(work_before, retained)?;
        Ok(document)
    }

    fn decode_prefab(
        &mut self,
        path: &str,
        bytes: &[u8],
    ) -> Result<PrefabDocument, ProjectContentError> {
        let work_before =
            self.reserve_work(scene_decode_work_plan(self.loader.limits.scene_file())?)?;
        let document = decode_prefab_candidate(path, bytes, self.loader.limits.scene_file())?
            .into_composition_source()
            .map_err(map_scene_publication_error)?;
        let retained = prefab_retained_bytes(&document)?
            .checked_add(ARC_CONTROL_BLOCK_BYTES)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
        self.commit_work_as_retained(work_before, retained)?;
        Ok(document)
    }

    fn collect_source_candidates(
        &mut self,
        document: &SceneDocument,
        assets: &DirectoryCapability,
        sources: &mut SourceCandidates,
    ) -> Result<(), ProjectContentError> {
        for entity in &document.entities {
            for (component, record) in &entity.components {
                let validation = self.validation;
                let schema = validation.bindings().schema(component).ok_or_else(|| {
                    map_asset_reference_error(DeclaredAssetReferenceError::UnknownComponent {
                        component: component.clone(),
                    })
                })?;
                let (references, reference_work) =
                    collect_effective_asset_references(schema, record, self.budget_mut())?;
                for reference in references {
                    self.add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
                    let (source, source_work) =
                        self.discover_source_reference(assets, reference.asset_ref())?;
                    let path = source.path();
                    self.observe_path(path.as_str())?;
                    if source.source_kind() == &AssetSourceKind::Scene {
                        if !reference.asset_ref().is_primary() {
                            return Err(unsupported_product_selection(reference.asset_ref()));
                        }
                        self.enqueue_scene(sources, &source)?;
                        drop(source);
                        self.subtract_budget(
                            ProjectContentBudgetKind::WorkBytes,
                            source_work.reserved,
                        );
                        continue;
                    }
                    if self.domains.supports(source.source_kind()) {
                        let bytes = require_asset_product(
                            &mut sources.products,
                            self.budget_mut(),
                            required_product(reference.asset_ref(), &source)?,
                        )?;
                        sources.product_work_bytes = sources
                            .product_work_bytes
                            .checked_add(bytes)
                            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
                    } else if !reference.asset_ref().is_primary() {
                        return Err(unsupported_product_selection(reference.asset_ref()));
                    }
                    let paths = match source.source_kind() {
                        AssetSourceKind::Image => &mut sources.images,
                        AssetSourceKind::Other(kind) if kind == "tileset" => &mut sources.tilesets,
                        kind if self.domains.supports(kind) => &mut sources.domains,
                        _ => {
                            return Err(ProjectContentError::with_identifier(
                                ProjectContentErrorKind::UnsupportedAssetKind,
                                "project.content.asset-kind-unsupported",
                                "Project Scene content references an unsupported asset kind",
                                "asset",
                                path.as_str(),
                            ));
                        }
                    };
                    let local_bytes = self
                        .asset_scope
                        .relative_path(path)
                        .ok_or_else(|| {
                            ProjectContentError::with_identifier(
                                ProjectContentErrorKind::HostAuthorityRejected,
                                "project.content.asset-source-outside-scope",
                                "Asset source is outside the admitted resource directory",
                                "asset",
                                path.as_str(),
                            )
                        })?
                        .len();
                    let queued_work = checked_plan_sum(
                        ProjectContentBudgetKind::WorkBytes,
                        [
                            size_of::<AssetPath>(),
                            BTREE_ENTRY_OVERHEAD,
                            local_bytes,
                            STRING_ALLOCATION_OVERHEAD,
                        ],
                    )?;
                    let queue_reservation = self.reserve_work(queued_work)?;
                    let local = self
                        .asset_scope
                        .relativize(path)
                        .expect("the borrowed source relation was checked above");
                    if !paths.contains(&local) {
                        self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
                        paths.insert(local);
                    } else {
                        drop(local);
                        self.subtract_budget(
                            ProjectContentBudgetKind::WorkBytes,
                            queue_reservation.reserved,
                        );
                    }
                    drop(source);
                    self.subtract_budget(ProjectContentBudgetKind::WorkBytes, source_work.reserved);
                }
                self.subtract_budget(ProjectContentBudgetKind::WorkBytes, reference_work);
            }
        }
        Ok(())
    }

    fn load_images(
        &mut self,
        assets: &DirectoryCapability,
        asset_root: &nara_project::ProjectPath,
        paths: BTreeSet<AssetPath>,
    ) -> Result<Vec<ProjectImageContent>, ProjectContentError> {
        let result_bytes = checked_plan_product(
            ProjectContentBudgetKind::WorkBytes,
            paths.len(),
            size_of::<ProjectImageContent>(),
        )?;
        let result_reservation = self.reserve_work(result_bytes)?;
        let mut images = Vec::new();
        images
            .try_reserve_exact(paths.len())
            .map_err(|_| allocation_failed("project.content.image-list-allocation-failed"))?;
        for path in paths {
            self.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            self.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
            let result = self.load_image(assets, asset_root, &path);
            self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            images.push(result?);
            let queued_work = queued_source_path_work_bytes(&path)?;
            drop(path);
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, queued_work);
        }
        self.commit_work_as_retained(result_reservation, result_bytes)?;
        Ok(images)
    }

    fn load_tilesets(
        &mut self,
        assets: &DirectoryCapability,
        asset_root: &nara_project::ProjectPath,
        paths: BTreeSet<AssetPath>,
    ) -> Result<(Vec<ProjectTileSetContent>, WorkReservation), ProjectContentError> {
        let result_bytes = checked_plan_product(
            ProjectContentBudgetKind::WorkBytes,
            paths.len(),
            size_of::<ProjectTileSetContent>(),
        )?;
        let mut result_reservation = self.reserve_work(result_bytes)?;
        let mut tilesets = Vec::new();
        tilesets
            .try_reserve_exact(paths.len())
            .map_err(|_| allocation_failed("project.content.tileset-list-allocation-failed"))?;
        for path in paths {
            self.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            self.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
            let result = self.load_tileset(assets, asset_root, &path);
            self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            let (tileset, paths_work) = result?;
            result_reservation.reserved = result_reservation
                .reserved
                .checked_add(paths_work.reserved)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
            tilesets.push(tileset);
            let queued_work = queued_source_path_work_bytes(&path)?;
            drop(path);
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, queued_work);
        }
        Ok((tilesets, result_reservation))
    }

    fn validate_tileset_image_extents(
        &self,
        tilesets: &[ProjectTileSetContent],
        images: &[ProjectImageContent],
    ) -> Result<(), ProjectContentError> {
        for tileset in tilesets {
            let image = images
                .iter()
                .find(|image| {
                    self.asset_scope.relative_path(image.path())
                        == Some(tileset.image_path.as_str())
                })
                .ok_or_else(|| {
                    ProjectContentError::with_identifier(
                        ProjectContentErrorKind::AssetMetaMismatch,
                        "project.content.tileset-image-missing",
                        "TileSet image dependency was not admitted",
                        "asset",
                        tileset.path.as_str(),
                    )
                })?;
            if tileset
                .source
                .validate_image_extent(image.image.extent())
                .is_err()
            {
                return Err(ProjectContentError::with_identifier(
                    ProjectContentErrorKind::AssetMetaMismatch,
                    "project.content.tileset-image-extent-mismatch",
                    "TileSet atlas extent does not match its decoded Image extent",
                    "asset",
                    tileset.path.as_str(),
                ));
            }
        }
        Ok(())
    }

    fn validate_tilemap_membership(
        &mut self,
        document: &SceneDocument,
        provenance: &SceneEntityProvenanceMap,
        tilesets: &[ProjectTileSetContent],
    ) -> Result<(), ProjectContentError> {
        for entity in &document.entities {
            let Some(record) = entity
                .components
                .get(&nara_reflect::ComponentTypeId::new("nara.tilemap.Tilemap"))
            else {
                continue;
            };
            let work = tilemap_membership_work_bytes(&record.value)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
            self.add_budget(ProjectContentBudgetKind::WorkBytes, work)?;
            let result = validate_persisted_tilemap_membership(&record.value, |reference| {
                tilesets
                    .iter()
                    .find(|tileset| match reference {
                        AssetRef::Path(path) => {
                            self.asset_scope.relative_path(&tileset.path) == Some(path.as_str())
                        }
                        AssetRef::StableId(id) => &tileset.stable_id == id,
                        AssetRef::Product(product) => {
                            reference.is_primary() && tileset.stable_id == product.source()
                        }
                    })
                    .map(|tileset| tileset.source.atlas)
            });
            self.subtract_budget(ProjectContentBudgetKind::WorkBytes, work);
            if let Err(error) = result {
                return Err(tilemap_membership_error(entity, provenance, error));
            }
        }
        Ok(())
    }

    fn commit_tileset_identities(
        &mut self,
        asset_root: &nara_project::ProjectPath,
        tilesets: &[ProjectTileSetContent],
    ) -> Result<(), ProjectContentError> {
        for tileset in tilesets {
            self.verify_catalogued_metadata(&tileset.path, asset_root, tileset.metadata_digest)?;
            let record = AssetRecord::new(
                tileset.stable_id,
                tileset.path.clone(),
                AssetSourceKind::Other("tileset".to_string()),
            );
            if let Some(indexed) = self.asset_database.record_for_path(&tileset.path) {
                if indexed != &record {
                    return Err(catalogue::identity_conflict(&tileset.path));
                }
            } else {
                self.reserve_asset_database_work(&record)?;
                self.asset_database
                    .insert(record)
                    .map_err(|_| catalogue::identity_conflict(&tileset.path))?;
            }
        }
        Ok(())
    }

    fn load_tileset(
        &mut self,
        assets: &DirectoryCapability,
        asset_root: &nara_project::ProjectPath,
        path: &AssetPath,
    ) -> Result<(ProjectTileSetContent, WorkReservation), ProjectContentError> {
        let meta_path = path.meta_path();
        let (meta_bytes, meta_digest) = self.read_file(
            assets,
            &meta_path,
            nara_asset::AssetMetaFileLimits::default()
                .encoded_bytes()
                .get(),
        )?;
        self.record_member(
            ProjectContentMemberKind::AssetMetadata,
            asset_root,
            &meta_path,
            meta_digest,
        )?;
        let (identity, ()) = AssetMetaCandidate::<()>::decode_json_bytes(&meta_bytes)
            .map_err(|_| {
                ProjectContentError::with_identifier(
                    ProjectContentErrorKind::AssetMeta,
                    "project.content.tileset-meta-invalid",
                    "TileSet asset metadata is invalid",
                    "asset",
                    path.as_str(),
                )
            })?
            .into_meta()
            .into_parts();
        if identity.path() != path {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.tileset-meta-path-mismatch",
                "TileSet asset metadata path does not match its source",
                "asset",
                path.as_str(),
            ));
        }
        if identity.source_kind() != &AssetSourceKind::Other("tileset".to_string()) {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::UnsupportedAssetKind,
                "project.content.tileset-kind-unsupported",
                "TileSet metadata does not declare the tileset source kind",
                "asset",
                path.as_str(),
            ));
        }
        let record_path_bytes = catalogue::scoped_path_bytes(&self.asset_scope, identity.path())?
            .checked_add(STRING_ALLOCATION_OVERHEAD)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let record_path_work = self.reserve_work(record_path_bytes)?;
        let record = identity.into_record(&self.asset_scope);
        self.verify_catalogued_metadata(record.path(), asset_root, meta_digest)?;
        let stable_id = record.stable_id();
        let source_ceiling = TileSetSourceFileLimits::default().encoded_bytes().get();
        let (source_bytes, source_digest) =
            self.read_file(assets, path.as_str(), source_ceiling)?;
        let source =
            TileSetSource::decode_ron_bytes(&source_bytes, TileSetSourceFileLimits::default())
                .map_err(|_| {
                    ProjectContentError::with_identifier(
                        ProjectContentErrorKind::AssetMeta,
                        "project.content.tileset-source-invalid",
                        "TileSet source data is invalid",
                        "asset",
                        path.as_str(),
                    )
                })?;
        let (image_path, image_path_work) =
            self.resolve_reference_path(&source.image, &AssetSourceKind::Image, asset_root)?;
        self.add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
        self.verify_catalogued_metadata(record.path(), asset_root, meta_digest)?;
        self.record_member(
            ProjectContentMemberKind::AssetSource,
            asset_root,
            path.as_str(),
            source_digest,
        )?;
        let path_work = self.reserve_work(string_retained_bytes(record.path().as_str())?)?;
        let path = record.path().clone();
        drop(record);
        self.subtract_budget(
            ProjectContentBudgetKind::WorkBytes,
            record_path_work.reserved,
        );
        let paths_work = WorkReservation {
            reserved: image_path_work
                .reserved
                .checked_add(path_work.reserved)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
        };
        Ok((
            ProjectTileSetContent {
                stable_id,
                path,
                image_path,
                metadata_digest: meta_digest,
                source_hash: source_digest,
                source,
            },
            paths_work,
        ))
    }

    fn load_image(
        &mut self,
        assets: &DirectoryCapability,
        asset_root: &nara_project::ProjectPath,
        path: &AssetPath,
    ) -> Result<ProjectImageContent, ProjectContentError> {
        let meta_path = path.meta_path();
        let (meta_bytes, meta_digest) = self.read_file(
            assets,
            &meta_path,
            nara_asset::AssetMetaFileLimits::default()
                .encoded_bytes()
                .get(),
        )?;
        self.record_member(
            ProjectContentMemberKind::AssetMetadata,
            asset_root,
            &meta_path,
            meta_digest,
        )?;
        let meta = AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(&meta_bytes)
            .map_err(|_| {
                ProjectContentError::with_identifier(
                    ProjectContentErrorKind::AssetMeta,
                    "project.content.asset-meta-invalid",
                    "Project asset metadata is invalid",
                    "asset",
                    path.as_str(),
                )
            })?
            .into_meta();
        if meta.identity().path() != path {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.asset-meta-path-mismatch",
                "Project asset metadata path does not match its source",
                "asset",
                path.as_str(),
            ));
        }
        if meta.identity().source_kind() != &AssetSourceKind::Image {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::UnsupportedAssetKind,
                "project.content.asset-kind-unsupported",
                "Project Scene content references an unsupported asset kind",
                "asset",
                path.as_str(),
            ));
        }
        let (identity, settings) = meta.into_parts();
        let record_path_bytes = catalogue::scoped_path_bytes(&self.asset_scope, identity.path())?
            .checked_add(STRING_ALLOCATION_OVERHEAD)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let record_path_work = self.reserve_work(record_path_bytes)?;
        let record = identity.into_record(&self.asset_scope);
        self.verify_catalogued_metadata(record.path(), asset_root, meta_digest)?;
        if let Some(indexed) = self.asset_database.record_for_path(record.path()) {
            if indexed != &record {
                return Err(catalogue::identity_conflict(path));
            }
        } else {
            self.reserve_asset_database_work(&record)?;
            self.asset_database.insert(record.clone()).map_err(|_| {
                ProjectContentError::with_identifier(
                    ProjectContentErrorKind::AssetMetaMismatch,
                    "project.content.asset-identity-conflict",
                    "Project asset identity conflicts within the Scene closure",
                    "asset",
                    path.as_str(),
                )
            })?;
        }
        let ceiling = self
            .loader
            .image_importer
            .limits()
            .max_encoded_bytes()
            .get();
        let (source_bytes, source_digest) = self.read_file(assets, path.as_str(), ceiling)?;
        let source_path_bytes = record.path().as_str().len();
        let import = self
            .loader
            .image_importer
            .preflight_unpublished_import(ImageBytesImportRequest::new(
                record,
                source_bytes,
                settings,
            ))
            .map_err(map_image_import_error)?;
        let memory = import.memory_plan();
        let work_before = self.reserve_image_memory(source_path_bytes, memory, record_path_work)?;
        let image = import.import().map_err(map_image_import_error)?;
        self.set_budget(ProjectContentBudgetKind::WorkBytes, work_before)?;
        self.record_member(
            ProjectContentMemberKind::AssetSource,
            asset_root,
            path.as_str(),
            source_digest,
        )?;
        Ok(ProjectImageContent { image })
    }

    fn reserve_image_memory(
        &mut self,
        path_bytes: usize,
        memory: ImageImportMemoryPlan,
        source_path_work: WorkReservation,
    ) -> Result<usize, ProjectContentError> {
        // The import takes the same canonical path allocation. Transfer its existing work
        // charge into the retained plan atomically with decoder and pixel admission.
        let work_before = self
            .budget_value(ProjectContentBudgetKind::WorkBytes)
            .checked_sub(source_path_work.reserved)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let work_bytes = work_before
            .checked_add(memory.decoder_work_bytes())
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let artifact_bytes = self
            .budget_value(ProjectContentBudgetKind::ArtifactBytes)
            .checked_add(memory.rgba_bytes())
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::ArtifactBytes))?;
        let retained_bytes = self
            .budget_value(ProjectContentBudgetKind::RetainedBytes)
            .checked_add(image_retained_plan_bytes(path_bytes, memory.rgba_bytes())?)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
        self.budget_mut()
            .set_many(&[
                (ProjectContentBudgetKind::WorkBytes, work_bytes),
                (ProjectContentBudgetKind::ArtifactBytes, artifact_bytes),
                (ProjectContentBudgetKind::RetainedBytes, retained_bytes),
            ])
            .map_err(ProjectContentError::budget)?;
        Ok(work_before)
    }

    fn observe_path(&mut self, path: &str) -> Result<RelativePath, ProjectContentError> {
        let invalid_path = || {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::InvalidLogicalPath,
                "project.content.path-invalid",
                "Project content path is invalid",
                "path",
                path,
            )
        };
        let preflight = RelativePath::preflight(path).map_err(|_| invalid_path())?;
        self.observe_depth(preflight.components())?;
        self.add_budget(
            ProjectContentBudgetKind::DirectoryEntries,
            preflight.components(),
        )?;
        self.add_budget(ProjectContentBudgetKind::PathBytes, path.len())?;
        RelativePath::new(path).map_err(|_| invalid_path())
    }

    fn observe_depth(&mut self, depth: usize) -> Result<(), ProjectContentError> {
        let observed = self
            .budget_value(ProjectContentBudgetKind::DirectoryDepth)
            .max(depth);
        self.set_budget(ProjectContentBudgetKind::DirectoryDepth, observed)
    }

    fn reserve_work(&mut self, amount: usize) -> Result<WorkReservation, ProjectContentError> {
        let previous = self.budget_value(ProjectContentBudgetKind::WorkBytes);
        let next = previous
            .checked_add(amount)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        self.set_budget(ProjectContentBudgetKind::WorkBytes, next)?;
        Ok(WorkReservation { reserved: amount })
    }

    fn remaining_expansion_work(
        &self,
        reserved: usize,
        upper: ByteLimit,
    ) -> Result<ByteLimit, ProjectContentError> {
        let snapshot = self.loader.budget_snapshot();
        let mut available = upper.get();
        for kind in [
            ProjectContentBudgetKind::WorkBytes,
            ProjectContentBudgetKind::AggregateBytes,
        ] {
            let other = snapshot
                .active(kind)
                .checked_sub(reserved)
                .ok_or_else(|| overflow_budget(kind))?;
            let limit = self.loader.limits.limit(kind);
            let remaining = limit
                .checked_sub(other)
                .and_then(ByteLimit::new)
                .ok_or_else(|| {
                    ProjectContentError::budget(ProjectContentBudgetError::synthetic(
                        kind,
                        other.saturating_add(1),
                        limit,
                    ))
                })?;
            available = available.min(remaining.get());
        }
        // Every operand is a nonzero ByteLimit; exhausted capacity was rejected above.
        ByteLimit::new(available)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))
    }

    fn resize_work(
        &mut self,
        reservation: &mut WorkReservation,
        amount: usize,
    ) -> Result<(), ProjectContentError> {
        let other_work = self
            .budget_value(ProjectContentBudgetKind::WorkBytes)
            .checked_sub(reservation.reserved)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        self.set_budget(
            ProjectContentBudgetKind::WorkBytes,
            other_work
                .checked_add(amount)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
        )?;
        reservation.reserved = amount;
        Ok(())
    }

    fn commit_work_as_retained(
        &mut self,
        reservation: WorkReservation,
        retained: usize,
    ) -> Result<(), ProjectContentError> {
        if retained > reservation.reserved {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::InvalidLimits,
                "project.content.memory-plan-insufficient",
                "Project content memory plan did not cover published residency",
            ));
        }
        let retained_bytes = self
            .budget_value(ProjectContentBudgetKind::RetainedBytes)
            .checked_add(retained)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
        let remaining_work = self
            .budget_value(ProjectContentBudgetKind::WorkBytes)
            .checked_sub(reservation.reserved)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        self.budget_mut()
            .set_many(&[
                (ProjectContentBudgetKind::WorkBytes, remaining_work),
                (ProjectContentBudgetKind::RetainedBytes, retained_bytes),
            ])
            .map_err(ProjectContentError::budget)?;
        Ok(())
    }

    fn record_member(
        &mut self,
        kind: ProjectContentMemberKind,
        directory: &nara_project::ProjectPath,
        path: &str,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let logical_path_bytes = directory
            .as_str()
            .len()
            .checked_add(1)
            .and_then(|length| length.checked_add(path.len()))
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::PathBytes))?;
        let work = content_closure_entry_work_bytes(logical_path_bytes)?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, work)?;
        self.content_closure
            .as_mut()
            .expect("load context owns one Scene closure builder until publication")
            .insert(kind, directory, path, digest)?;
        self.content_closure_work_bytes = self
            .content_closure_work_bytes
            .checked_add(work)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        Ok(())
    }

    fn record_project_member(
        &mut self,
        kind: ProjectContentMemberKind,
        path: nara_project::ProjectPath,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let work = content_closure_entry_work_bytes(path.as_str().len())?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, work)?;
        self.content_closure
            .as_mut()
            .expect("load context owns one Scene closure builder until publication")
            .insert_project_path(kind, path, digest)?;
        self.content_closure_work_bytes = self
            .content_closure_work_bytes
            .checked_add(work)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        Ok(())
    }

    fn finish_content_digest(&mut self) -> Result<ContentDigest, ProjectContentError> {
        let content_closure = self
            .content_closure
            .as_ref()
            .expect("load context owns one Scene closure builder until publication");
        let frame_work = content_closure.content_digest_framed_bytes()?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, frame_work)?;
        let digest = self
            .content_closure
            .as_ref()
            .expect("load context owns one Scene closure builder until publication")
            .content_digest(frame_work)?;
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, frame_work);
        Ok(digest)
    }

    fn add_budget(
        &mut self,
        kind: ProjectContentBudgetKind,
        amount: usize,
    ) -> Result<(), ProjectContentError> {
        self.budget_mut()
            .add(kind, amount)
            .map_err(ProjectContentError::budget)
    }

    fn set_budget(
        &mut self,
        kind: ProjectContentBudgetKind,
        value: usize,
    ) -> Result<(), ProjectContentError> {
        self.budget_mut()
            .set(kind, value)
            .map_err(ProjectContentError::budget)
    }

    fn budget_value(&self, kind: ProjectContentBudgetKind) -> usize {
        self.budget
            .as_ref()
            .expect("load context budget is present before publication")
            .value(kind)
    }

    fn budget_mut(&mut self) -> &mut BudgetTicket {
        self.budget
            .as_mut()
            .expect("load context budget is present before publication")
    }

    fn subtract_budget(&mut self, kind: ProjectContentBudgetKind, amount: usize) {
        self.budget_mut().subtract(kind, amount);
    }
}

fn fs_error_is_not_found(error: &FsError) -> bool {
    matches!(
        error,
        FsError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound
    )
}

fn decode_scene_candidate(
    path: &str,
    bytes: &[u8],
    limits: nara_scene::SceneFileLimits,
) -> Result<SceneDocumentCandidate, ProjectContentError> {
    if path.ends_with(".ron") {
        SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
    } else if path.ends_with(".json") {
        SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
    } else {
        return Err(ProjectContentError::with_identifier(
            ProjectContentErrorKind::SceneFormat,
            "project.content.scene-encoding-unsupported",
            "Project scene encoding is unsupported",
            "scene",
            path,
        ));
    }
    .map_err(|error| map_scene_format_error(path, error))
}

fn decode_prefab_candidate(
    path: &str,
    bytes: &[u8],
    limits: nara_scene::SceneFileLimits,
) -> Result<PrefabDocumentCandidate, ProjectContentError> {
    if path.ends_with(".ron") {
        PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
    } else if path.ends_with(".json") {
        PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
    } else {
        return Err(ProjectContentError::with_identifier(
            ProjectContentErrorKind::SceneFormat,
            "project.content.prefab-encoding-unsupported",
            "Project prefab encoding is unsupported",
            "prefab",
            path,
        ));
    }
    .map_err(|error| map_prefab_format_error(path, error))
}

fn map_scene_format_error(path: &str, error: SceneFormatError) -> ProjectContentError {
    let (code, summary) = match &error {
        SceneFormatError::LegacyPlacementShape { .. } => (
            "project.content.scene-legacy-placement-shape",
            "Rewrite the legacy scene with ordered roots and parent-owned children; sibling order cannot be inferred",
        ),
        _ => (
            "project.content.scene-format-invalid",
            "Project scene format is invalid",
        ),
    };
    map_format_error(path, "scene", code, summary, &error)
}

fn map_prefab_format_error(path: &str, error: SceneFormatError) -> ProjectContentError {
    let (code, summary) = match &error {
        SceneFormatError::LegacyPlacementShape { .. } => (
            "project.content.prefab-legacy-placement-shape",
            "Rewrite the legacy prefab with ordered roots and parent-owned children; sibling order cannot be inferred",
        ),
        _ => (
            "project.content.prefab-format-invalid",
            "Project prefab format is invalid",
        ),
    };
    map_format_error(path, "prefab", code, summary, &error)
}

fn map_format_error(
    path: &str,
    source_key: &'static str,
    code: &'static str,
    summary: &'static str,
    error: &SceneFormatError,
) -> ProjectContentError {
    let diagnostic = crate::scene_format_diagnostic::scene_format_diagnostic(code, summary, error);
    let source = PublicDiagnosticIdentifier::new(path).map_or_else(
        |_| DiagnosticField::sensitive(field_key(source_key)),
        |value| DiagnosticField::public_identifier(field_key(source_key), value),
    );
    ProjectContentError::with_report(
        ProjectContentErrorKind::SceneFormat,
        single_report(attach_field(diagnostic, source)),
    )
}

fn map_scene_publication_error(
    error: nara_scene::SceneFilePublicationError,
) -> ProjectContentError {
    map_scene_diagnostics(
        ProjectContentErrorKind::ScenePublication,
        error.into_diagnostics(),
    )
}

fn map_scene_diagnostics(
    default_kind: ProjectContentErrorKind,
    diagnostics: DiagnosticReport,
) -> ProjectContentError {
    let kind = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code().as_str() == "scene.hierarchy-projection-unavailable")
    {
        ProjectContentErrorKind::UnsupportedHierarchySemantics
    } else {
        default_kind
    };
    ProjectContentError::with_report(kind, diagnostics)
}

fn collect_effective_asset_references(
    schema: &nara_reflect::ComponentSchema,
    record: &nara_scene::SceneComponentRecord,
    budget: &mut BudgetTicket,
) -> Result<(Vec<DeclaredAssetReference>, usize), ProjectContentError> {
    let mut work_bytes = 0usize;
    let references = collect_declared_effective_asset_references_with_admission(
        schema,
        record.version,
        &record.value,
        EntityReferenceTraversalLimits::default(),
        |_, _, allocation| {
            budget
                .add(ProjectContentBudgetKind::WorkBytes, allocation)
                .map_err(ProjectContentError::budget)?;
            work_bytes = work_bytes
                .checked_add(allocation)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
            Ok(())
        },
    )
    .map_err(|error| match error {
        AssetReferenceCollectionError::Reference(error) => map_asset_reference_error(error),
        AssetReferenceCollectionError::Admission(error) => error,
    })?;
    Ok((references, work_bytes))
}

fn required_product(
    reference: &AssetRef,
    source: &AssetRecord,
) -> Result<AssetProductRef, ProjectContentError> {
    match reference {
        AssetRef::Product(reference) => Ok(*reference),
        AssetRef::Path(_) | AssetRef::StableId(_) => {
            AssetProductRef::new(source.stable_id(), ImportedProductId::PRIMARY)
                .map_err(|_| catalogue::identity_conflict(source.path()))
        }
    }
}

fn require_asset_product(
    required: &mut RequiredAssetProducts,
    budget: &mut BudgetTicket,
    product: AssetProductRef,
) -> Result<usize, ProjectContentError> {
    let selected = required.get(&product.source());
    if selected.is_some_and(|products| products.contains(&product)) {
        return Ok(0);
    }
    let bytes = checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            size_of::<AssetProductRef>() + BTREE_ENTRY_OVERHEAD,
            if selected.is_none() {
                size_of::<(StableAssetId, BTreeSet<AssetProductRef>)>() + BTREE_ENTRY_OVERHEAD
            } else {
                0
            },
        ],
    )?;
    budget
        .add(ProjectContentBudgetKind::WorkBytes, bytes)
        .map_err(ProjectContentError::budget)?;
    required
        .entry(product.source())
        .or_default()
        .insert(product);
    Ok(bytes)
}

fn unsupported_product_selection(reference: &AssetRef) -> ProjectContentError {
    ProjectContentError::with_identifier(
        ProjectContentErrorKind::AssetReference,
        "project.content.asset-product-unsupported",
        "This asset source consumer supports only its primary product",
        "asset-product",
        &reference.to_string(),
    )
}

fn tilemap_membership_error(
    entity: &SceneEntityRecord,
    provenance: &SceneEntityProvenanceMap,
    error: TilemapMembershipError,
) -> ProjectContentError {
    let (code, summary, tileset, coord, tile) = match error {
        TilemapMembershipError::InvalidValue(_error) => (
            "project.content.tilemap-value-invalid",
            "Tilemap component value is invalid",
            None,
            None,
            None,
        ),
        TilemapMembershipError::TileSetUnavailable { tileset } => (
            "project.content.tileset-unavailable",
            "TileSet dependency was not admitted",
            Some(tileset),
            None,
            None,
        ),
        TilemapMembershipError::TileOutsideAtlas {
            tileset,
            coord,
            tile,
        } => (
            "project.content.tile-index-out-of-range",
            "Tilemap cell refers to a tile outside its admitted TileSet atlas",
            Some(tileset),
            Some(coord),
            Some(tile),
        ),
    };
    let mut diagnostic = diagnostic(code, summary);
    diagnostic = attach_field(
        diagnostic,
        DiagnosticField::public_identifier(
            field_key("entity-id"),
            PublicDiagnosticIdentifier::new(entity.id.as_str()).unwrap_or_else(|_| {
                PublicDiagnosticIdentifier::new("invalid-entity-id").expect("literal is valid")
            }),
        ),
    );
    if let Some(source) = provenance.get(&entity.id) {
        diagnostic = attach_field(
            diagnostic,
            DiagnosticField::public_identifier(
                field_key("source-entity-id"),
                PublicDiagnosticIdentifier::new(source.local_entity_id().as_str()).unwrap_or_else(
                    |_| {
                        PublicDiagnosticIdentifier::new("invalid-source-entity-id")
                            .expect("literal is valid")
                    },
                ),
            ),
        );
    }
    if let Some(tileset) = tileset {
        diagnostic = attach_field(
            diagnostic,
            DiagnosticField::project_relative(field_key("tileset"), tileset.to_string().as_str())
                .unwrap_or_else(|_| DiagnosticField::sensitive(field_key("tileset"))),
        );
    }
    if let Some(coord) = coord {
        diagnostic = attach_field(
            diagnostic,
            DiagnosticField::public_i64(field_key("x"), i64::from(coord.x)),
        );
        diagnostic = attach_field(
            diagnostic,
            DiagnosticField::public_i64(field_key("y"), i64::from(coord.y)),
        );
    }
    if let Some(tile) = tile {
        diagnostic = attach_field(
            diagnostic,
            DiagnosticField::public_u64(field_key("tile"), u64::from(tile.raw())),
        );
    }
    let mut report = DiagnosticReport::default();
    report.push(diagnostic);
    ProjectContentError::with_report(ProjectContentErrorKind::ScenePublication, report)
}

fn content_revision(
    lineage: ProjectSettingsLineage,
    schema: SchemaCompositionFingerprint,
    content: ContentDigest,
    selected_scene_path: &nara_project::ProjectPath,
) -> Result<ProjectContentRevision, ProjectContentError> {
    let mut framed = Vec::with_capacity(160);
    framed.extend_from_slice(b"nara.project-content-revision.v2\0");
    framed.extend_from_slice(&lineage.settings_digest());
    if let Some(root) = lineage.project_root_identity() {
        framed.extend_from_slice(&root.session().get().to_le_bytes());
        framed.extend_from_slice(&root.generation().get().to_le_bytes());
    }
    append_frame(&mut framed, schema.to_hex().as_bytes())?;
    framed.extend_from_slice(&content.length().to_le_bytes());
    framed.extend_from_slice(content.as_bytes());
    append_frame(&mut framed, selected_scene_path.as_str().as_bytes())?;
    Ok(ProjectContentRevision(
        *ContentDigest::of_bytes(&framed).as_bytes(),
    ))
}

fn append_frame(output: &mut Vec<u8>, value: &[u8]) -> Result<(), ProjectContentError> {
    let length = u64::try_from(value.len())
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn content_closure_entry_work_bytes(path_bytes: usize) -> Result<usize, ProjectContentError> {
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            size_of::<ProjectContentMemberReceipt>(),
            BTREE_ENTRY_OVERHEAD,
            path_bytes
                .checked_add(STRING_ALLOCATION_OVERHEAD)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?,
        ],
    )
}

const SNAPSHOT_LEASE_OVERHEAD: usize = 128;
const IMAGE_RETENTION_OWNER_OVERHEAD: usize =
    ARC_CONTROL_BLOCK_BYTES + size_of::<ProjectImageRetention>();
const IMAGE_METADATA_OVERHEAD: usize = 256;
const ENTITY_WORK_BYTES: usize = 128;
const COMPONENT_WORK_BYTES: usize = 96;
const PREFAB_INSTANCE_WORK_BYTES: usize = 128;
const OVERLAY_DECLARATION_WORK_BYTES: usize = 256;
const DIAGNOSTIC_SOURCE_WORK_BYTES: usize = 32;
const SHAPE_NODE_WORK_BYTES: usize = 32;
const SHAPE_ITEM_WORK_BYTES: usize = 64;

fn scene_decode_work_plan(
    limits: nara_scene::SceneFileLimits,
) -> Result<usize, ProjectContentError> {
    let shape = limits.shape();
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            limits.encoded_bytes().get(),
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                shape.nodes().get(),
                SHAPE_NODE_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                shape.container_items().get(),
                SHAPE_ITEM_WORK_BYTES,
            )?,
            shape.total_string_bytes().get(),
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.entities().get(),
                ENTITY_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.components().get(),
                COMPONENT_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.prefab_instances().get(),
                PREFAB_INSTANCE_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.overlay_declarations().get(),
                OVERLAY_DECLARATION_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.diagnostic_sources().get(),
                DIAGNOSTIC_SOURCE_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.component_value_nodes().get(),
                VALUE_NODE_ALLOCATION_BYTES,
            )?,
            limits.component_value_bytes().get(),
        ],
    )
}

fn prefab_expansion_work_plan(
    limits: nara_scene::PrefabExpansionLimits,
) -> Result<usize, ProjectContentError> {
    // Scene owns the same allocation model for full admission and suspended callbacks.
    limits
        .checked_work_bytes()
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))
}

pub(crate) fn startup_scene_retained_bytes(
    candidate: &ExpandedSceneCandidate,
) -> Result<usize, ProjectContentError> {
    crate::scene_retention::startup_scene_retained_bytes(candidate)
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn editor_startup_scene_retained_bytes(
    candidate: &ExpandedSceneCandidate,
) -> Result<usize, ProjectContentError> {
    crate::scene_retention::direct_startup_scene_retained_bytes(candidate)
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?
        .checked_add(SNAPSHOT_LEASE_OVERHEAD)
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn scene_retained_bytes(document: &SceneDocument) -> Result<usize, ProjectContentError> {
    crate::scene_retention::scene_retained_bytes(document)
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn prefab_retained_bytes(document: &PrefabDocument) -> Result<usize, ProjectContentError> {
    crate::scene_retention::prefab_retained_bytes(document)
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn string_retained_bytes(value: &str) -> Result<usize, ProjectContentError> {
    crate::scene_retention::string_retained_bytes(value)
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn checked_plan_product(
    kind: ProjectContentBudgetKind,
    count: usize,
    bytes: usize,
) -> Result<usize, ProjectContentError> {
    count
        .checked_mul(bytes)
        .ok_or_else(|| overflow_budget(kind))
}

fn checked_plan_sum<const N: usize>(
    kind: ProjectContentBudgetKind,
    values: [usize; N],
) -> Result<usize, ProjectContentError> {
    values.into_iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| overflow_budget(kind))
    })
}

fn checked_add_to(
    total: &mut usize,
    value: usize,
    kind: ProjectContentBudgetKind,
) -> Result<(), ProjectContentError> {
    *total = total
        .checked_add(value)
        .ok_or_else(|| overflow_budget(kind))?;
    Ok(())
}

fn image_retained_plan_bytes(
    path_bytes: usize,
    rgba_bytes: usize,
) -> Result<usize, ProjectContentError> {
    rgba_bytes
        .checked_add(
            STRING_ALLOCATION_OVERHEAD
                .checked_add(path_bytes)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?,
        )
        .and_then(|bytes| bytes.checked_add(ARC_CONTROL_BLOCK_BYTES))
        .and_then(|bytes| bytes.checked_add(IMAGE_METADATA_OVERHEAD))
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn queued_source_path_work_bytes(path: &AssetPath) -> Result<usize, ProjectContentError> {
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            size_of::<AssetPath>(),
            BTREE_ENTRY_OVERHEAD,
            string_retained_bytes(path.as_str())?,
        ],
    )
}

fn component_bindings_witness_retained_bytes(
    bindings: &SceneComponentBindings,
) -> Result<usize, ProjectContentError> {
    // The native witness is weak. Charge the complete data-owner metadata even when its Arcs
    // are shared with the plan; encoded size times sixteen covers tree and allocation overhead.
    // Snapshot and escaped Scene sources share one witness allocation and one metadata charge.
    // Count through a sink so estimating retained metadata needs no temporary encoded buffer.
    let mut encoded = SchemaMetadataSize(0);
    for owner in bindings.data_owners() {
        serde_json::to_writer(
            &mut encoded,
            &(owner.owner_id(), owner.current(), owner.predecessor()),
        )
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
    }
    checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [
            size_of::<SceneComponentBindingsWitness>(),
            ARC_CONTROL_BLOCK_BYTES,
            checked_plan_product(ProjectContentBudgetKind::RetainedBytes, encoded.0, 16)?,
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                bindings.data_owners().len(),
                ARC_CONTROL_BLOCK_BYTES
                    + size_of::<(
                        nara_reflect::PersistentComponentSchemaOwnerId,
                        nara_reflect::ComponentSchemaOwnerSnapshot,
                    )>(),
            )?,
        ],
    )
}

struct SchemaMetadataSize(usize);

impl io::Write for SchemaMetadataSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("schema metadata size overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn snapshot_retained_overhead(
    selected_scene_source: &SourceDocumentKey,
    scenes: &[ProjectSceneContent],
    prefabs: &BTreeMap<AssetSourcePath, LoadedPrefabContent>,
    images: &[ProjectImageContent],
    tilesets: &[ProjectTileSetContent],
) -> Result<usize, ProjectContentError> {
    let mut total = checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [
            size_of::<ProjectContentSnapshotInner>(),
            ARC_CONTROL_BLOCK_BYTES,
            3 * ARC_CONTROL_BLOCK_BYTES,
            size_of::<Arc<SceneDocument>>(),
            SNAPSHOT_LEASE_OVERHEAD,
            GENERATION_LEASE_RETAINED_BYTES,
            if images.is_empty() {
                0
            } else {
                IMAGE_RETENTION_OWNER_OVERHEAD
            },
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                prefabs.len(),
                size_of::<ProjectPrefabContent>(),
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                images.len(),
                size_of::<ProjectImageContent>(),
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                tilesets.len(),
                size_of::<ProjectTileSetContent>(),
            )?,
        ],
    )?;
    for prefab in prefabs.values() {
        checked_add_to(
            &mut total,
            string_retained_bytes(prefab.path.as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    checked_add_to(
        &mut total,
        selected_scene_source.retained_bytes(),
        ProjectContentBudgetKind::RetainedBytes,
    )?;
    for scene in scenes {
        checked_add_to(
            &mut total,
            scene.source.retained_bytes(),
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    for prefab in prefabs.values() {
        checked_add_to(
            &mut total,
            prefab.source.retained_bytes(),
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    for tileset in tilesets {
        // Canonical and resolved Image paths transferred their work reservations above.
        if let AssetRef::Path(path) = &tileset.source.image {
            checked_add_to(
                &mut total,
                string_retained_bytes(path.as_str())?,
                ProjectContentBudgetKind::RetainedBytes,
            )?;
        }
    }
    Ok(total)
}

fn content_closure_retained_bytes(
    content_closure: &ProjectContentClosureDraft,
) -> Result<usize, ProjectContentError> {
    let mut total = checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [
            size_of::<closure::ProjectContentClosureReceiptInner>(),
            ARC_CONTROL_BLOCK_BYTES,
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                content_closure.members().len(),
                size_of::<ProjectContentMemberReceipt>(),
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                content_closure.runtime_directories().len(),
                size_of::<nara_project::ProjectPath>(),
            )?,
        ],
    )?;
    checked_add_to(
        &mut total,
        string_retained_bytes(content_closure.selected_scene_path().as_str())?,
        ProjectContentBudgetKind::RetainedBytes,
    )?;
    for path in content_closure.runtime_directories() {
        checked_add_to(
            &mut total,
            string_retained_bytes(path.as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    for member in content_closure.members() {
        checked_add_to(
            &mut total,
            string_retained_bytes(member.path().as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    Ok(total)
}

fn allocation_failed(code: &'static str) -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::AllocationFailed,
        code,
        "Project content allocation failed after budget admission",
    )
}

fn map_fs_error(error: FsError) -> ProjectContentError {
    match error {
        FsError::Unsupported { operation, .. } => fs_authority_error(
            ProjectContentErrorKind::HostAuthorityUnsupported,
            "project.content.authority-unsupported",
            "Project content authority is unsupported",
            operation,
        ),
        FsError::Unproven { operation, .. } => fs_authority_error(
            ProjectContentErrorKind::HostAuthorityUnproven,
            "project.content.authority-unproven",
            "Project content authority cannot prove the required invariant",
            operation,
        ),
        FsError::Io { operation, source } => {
            let diagnostic = attach_field(
                attach_field(
                    diagnostic("project.content.host-io", "Project content host I/O failed"),
                    DiagnosticField::public_identifier(
                        field_key("operation"),
                        PublicDiagnosticIdentifier::new(fs_operation_id(operation))
                            .expect("filesystem operation IDs are engine-owned"),
                    ),
                ),
                DiagnosticField::public_identifier(
                    field_key("io_kind"),
                    PublicDiagnosticIdentifier::new(io_error_kind_id(source.kind()))
                        .expect("I/O kind IDs are engine-owned"),
                ),
            );
            ProjectContentError::with_report(
                ProjectContentErrorKind::HostIo,
                single_report(with_sensitive(diagnostic, "source")),
            )
        }
        FsError::ByteLimitExceeded { limit } => {
            let error = ProjectContentBudgetError::synthetic(
                ProjectContentBudgetKind::EncodedBytes,
                usize::try_from(limit.saturating_add(1)).unwrap_or(usize::MAX),
                usize::try_from(limit).unwrap_or(usize::MAX),
            );
            ProjectContentError::budget(error)
        }
        FsError::DirectoryEnumerationEntryLimitExceeded { limit }
        | FsError::DirectoryEnumerationRejectedFactLimitExceeded { limit } => {
            ProjectContentError::budget(ProjectContentBudgetError::synthetic(
                ProjectContentBudgetKind::DirectoryEntries,
                limit.saturating_add(1),
                limit,
            ))
        }
        _ => ProjectContentError::single(
            ProjectContentErrorKind::HostAuthorityRejected,
            "project.content.authority-rejected",
            "Project content authority was rejected",
        ),
    }
}

fn map_image_importer_create_error(_error: ImageImporterCreateError) -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::InvalidLimits,
        "project.content.image-limits-invalid",
        "Project image import limits are invalid",
    )
}

fn map_image_import_error(error: ImageImportError) -> ProjectContentError {
    let diagnostic = attach_field(
        diagnostic(
            "project.content.image-import-failed",
            "Project image import failed",
        ),
        DiagnosticField::public_identifier(
            field_key("stage"),
            PublicDiagnosticIdentifier::new(error.stage().as_str())
                .expect("image import stages are engine-owned"),
        ),
    );
    ProjectContentError::with_report(
        ProjectContentErrorKind::ImageImport,
        single_report(diagnostic),
    )
}

fn map_asset_reference_error(_error: DeclaredAssetReferenceError) -> ProjectContentError {
    ProjectContentError::single(
        ProjectContentErrorKind::AssetReference,
        "project.content.asset-reference-invalid",
        "Project component asset reference is invalid",
    )
}

fn overflow_budget(kind: ProjectContentBudgetKind) -> ProjectContentError {
    ProjectContentError::budget(ProjectContentBudgetError::synthetic(
        kind,
        usize::MAX,
        usize::MAX,
    ))
}

fn fs_authority_error(
    kind: ProjectContentErrorKind,
    code: &'static str,
    summary: &'static str,
    operation: FsOperation,
) -> ProjectContentError {
    let diagnostic = attach_field(
        diagnostic(code, summary),
        DiagnosticField::public_identifier(
            field_key("operation"),
            PublicDiagnosticIdentifier::new(fs_operation_id(operation))
                .expect("filesystem operation IDs are engine-owned"),
        ),
    );
    ProjectContentError::with_report(kind, single_report(with_sensitive(diagnostic, "source")))
}

fn diagnostic(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("project content diagnostic codes are engine-owned"),
        SafeSummary::new(summary).expect("project content summaries are engine-owned"),
    )
}

fn field_key(key: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(key).expect("project content field keys are engine-owned")
}

fn attach_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("project content diagnostics use unique bounded fields")
}

fn with_sensitive(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    attach_field(diagnostic, DiagnosticField::sensitive(field_key(key)))
}

fn single_report(diagnostic: Diagnostic) -> DiagnosticReport {
    let mut report = DiagnosticReport::default();
    report.push(diagnostic);
    report
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
    use nara_asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        AssetSourcePath, ImportedProductId, StableAssetId,
    };
    use nara_core::ByteLimit;
    use nara_fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions, RelativePath,
        TrustMode,
    };
    use nara_identity::SceneEntityId;
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        PrefabAddition, PrefabComponentIntent, PrefabDocument, PrefabEntityTarget, PrefabInstance,
        PrefabInstanceClaim, PrefabOverlay, PrefabPlacement, PrefabPlacementParent,
        PrefabPlacementSlot, SceneComponentBindings, SceneComponentRecord, SceneDocument,
        SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
    };

    use crate::project_host::{
        ProjectSettingsCandidate, RuntimePlan, ingest_project_manifest, resolve_product_recipe,
        runtime_agreement::prepare_runtime_schema_agreement,
    };
    use crate::{ProductRecipe, SchemaContribution};

    use super::{
        LoadContext, ProjectContentBudgetKind, ProjectContentErrorKind, ProjectContentLimits,
        ProjectContentLoader, ProjectContentMemberKind, component_bindings_witness_retained_bytes,
    };

    #[test]
    fn runtime_content_loads_mixed_root_records_and_data_declared_assets() {
        let project = LazyContentProject::new();
        let owner = data_owner();
        let data = ComponentTypeId::new("test.content.Image");
        let record = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("data.png".into())),
            ]),
        )
        .with_owner(owner.owner_id().clone());
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("mixed").unwrap(),
        )
        .with_component(data.clone(), record.clone())
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Mixed root".into()),
            ),
        )]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("assets/data.png"),
            include_bytes!("../tests/fixtures/images/valid-rgba-1x1.png"),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("98dd688f-1cb5-42be-b9f2-f580ca0778af").unwrap(),
                AssetPath::new("data.png").unwrap(),
                AssetSourceKind::Image,
            ),
            nara_image::ImageImportSettings::default(),
        );
        fs::write(
            project.root.join("assets/data.png.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        let (candidate, plan, loader) = project.inputs_with_recipe(data_recipe(&owner));
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert_eq!(
            snapshot.scene(),
            &document,
            "the captured root remains authored",
        );
        assert_eq!(
            snapshot.expanded_scene().entities[0].components[&data],
            record
        );
        assert_eq!(snapshot.images().len(), 1);
        assert!(!snapshot.source_upgrade_required());
        assert!(snapshot.content_closure().members().iter().any(|member| {
            member.kind() == ProjectContentMemberKind::AssetSource
                && member.path().as_str() == "assets/data.png"
        }));
        let bindings = plan.schema_validation().bindings();
        let shared =
            SceneComponentBindings::new(bindings.native_snapshot().clone(), [owner]).unwrap();
        let independent =
            SceneComponentBindings::new(bindings.native_snapshot().clone(), [data_owner()])
                .unwrap();
        assert!(snapshot.shares_component_bindings(&shared));
        assert_eq!(
            bindings.composition_fingerprint(),
            independent.composition_fingerprint(),
        );
        assert!(!snapshot.shares_component_bindings(&independent));
        let baseline = loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes);
        let source = snapshot
            .prepare_editor_startup_scene(
                snapshot.selected_scene_source_key(),
                snapshot.scene(),
                &shared,
            )
            .unwrap();
        assert_eq!(
            loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::RetainedBytes),
            baseline + source.retained_bytes(),
        );
        drop(source);
        assert_eq!(
            snapshot
                .prepare_editor_startup_scene(
                    snapshot.selected_scene_source_key(),
                    snapshot.scene(),
                    &independent,
                )
                .unwrap_err()
                .kind(),
            ProjectContentErrorKind::SchemaMismatch,
        );
        assert_eq!(
            loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::RetainedBytes),
            baseline,
        );
        let metadata_bytes = component_bindings_witness_retained_bytes(bindings).unwrap();
        assert!(metadata_bytes > 0);
        assert!(
            loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::RetainedBytes)
                >= metadata_bytes
        );
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        // The path-only load above must avoid the deliberately invalid unused metadata. Explicit
        // stable product selection now requires a complete valid identity catalogue instead.
        fs::write(
            project.root.join("assets/unused.png.meta"),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str("706fd198-2412-4ccb-a398-f9f826fbe208").unwrap(),
                    AssetPath::new("unused.png").unwrap(),
                    AssetSourceKind::Image,
                ),
                nara_image::ImageImportSettings::default(),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        for product in [
            nara_asset::ImportedProductId::PRIMARY,
            nara_asset::ImportedProductId::parse_subproduct("dd195b6a-449c-4e1a-bec5-24aa8d83870c")
                .unwrap(),
        ] {
            let reference =
                nara_asset::AssetProductRef::new(metadata.identity().stable_id(), product).unwrap();
            let mut selected = document.clone();
            selected.entities[0]
                .components
                .get_mut(&data)
                .unwrap()
                .value = nara_reflect::encode_asset_reference_value(&AssetRef::Product(reference));
            fs::write(
                project.root.join("scenes/startup.scene.json"),
                selected.to_json_string().unwrap(),
            )
            .unwrap();
            if product == nara_asset::ImportedProductId::PRIMARY {
                let selected = loader.load(&candidate, &plan).unwrap();
                assert_eq!(selected.images().len(), 1);
            } else {
                let rejected = loader.load(&candidate, &plan).unwrap_err();
                assert_eq!(rejected.kind(), ProjectContentErrorKind::AssetReference);
                assert!(rejected.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "project.content.asset-product-unsupported"
                }));
            }
            assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        }
        let limits = ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::RetainedBytes, metadata_bytes - 1)
            .unwrap();
        let short = ProjectContentLoader::with_limits(loader.project_root(), limits).unwrap();
        assert_eq!(
            short.load(&candidate, &plan).unwrap_err().kind(),
            ProjectContentErrorKind::BudgetExceeded,
        );
        assert_eq!(
            short
                .budget_snapshot()
                .high_water(ProjectContentBudgetKind::Files),
            0,
        );
        assert_eq!(short.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn runtime_content_rejects_unselected_or_invalid_data_root_records() {
        let project = LazyContentProject::new();
        let owner = data_owner();
        for case in [
            "unselected",
            "missing-owner",
            "wrong-owner",
            "future",
            "invalid",
        ] {
            let mut record =
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7))
                    .with_owner(owner.owner_id().clone());
            match case {
                "missing-owner" => record.owner = None,
                "wrong-owner" => {
                    record.owner =
                        Some(PersistentComponentSchemaOwnerId::try_new("test.other").unwrap());
                }
                "future" => record.version = ComponentSchemaVersion::new(2).unwrap(),
                "invalid" => record.value = ComponentValue::Bool(true),
                _ => {}
            }
            let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("data").unwrap(),
            )
            .with_component(ComponentTypeId::new("test.content.Score"), record)]);
            fs::write(
                project.root.join("scenes/startup.scene.json"),
                document.to_json_string().unwrap(),
            )
            .unwrap();
            let recipe = if case == "unselected" {
                ProductRecipe::new()
            } else {
                data_recipe(&owner)
            };
            let (candidate, plan, loader) = project.inputs_with_recipe(recipe);
            let error = loader.load(&candidate, &plan).unwrap_err();
            assert!(error.diagnostics().has_errors(), "{case}");
            assert_eq!(loader.budget_snapshot().active_reservations(), 0, "{case}");
        }
    }

    #[test]
    fn runtime_agreement_checks_selected_data_catalogs_and_exact_owner_witness() {
        let project = LazyContentProject::new();
        let manifest = project.root.join("nara.toml");
        let source = fs::read_to_string(&manifest).unwrap();
        fs::write(
            manifest,
            format!("{source}\n[authoring]\nschema_bundle = \"schema.bundle.json\"\n"),
        )
        .unwrap();
        let owner = data_owner();
        let (candidate, plan, loader) = project.inputs_with_recipe(data_recipe(&owner));
        let bundle = plan.schema_validation().authoring_schema_bundle();
        for case in ["matching", "missing", "divergent"] {
            let mut owners = bundle
                .owners()
                .iter()
                .filter(|selected| selected.owner_id() != owner.owner_id())
                .map(AuthoringSchemaBundleOwner::from)
                .collect::<Vec<_>>();
            if case != "missing" {
                let mut current = owner.current().clone();
                if case == "divergent" {
                    current.components[0].aliases.push("Changed".into());
                }
                owners.push(AuthoringSchemaBundleOwner::new(
                    owner.owner_id().clone(),
                    current,
                    owner.predecessor().cloned(),
                ));
            }
            let bundle = AuthoringSchemaBundle::new(owners).unwrap();
            fs::write(
                project.root.join("schema.bundle.json"),
                bundle.to_json_string().unwrap(),
            )
            .unwrap();
            let result =
                prepare_runtime_schema_agreement(&loader.project_root(), &candidate, &plan);
            if case == "matching" {
                let agreement = result.unwrap();
                assert!(agreement.matches_inputs(
                    loader.project_root().identity(),
                    &candidate,
                    &plan,
                ));
                let (_, independent, _) = project.inputs_with_recipe(data_recipe(&data_owner()));
                assert!(!agreement.matches_inputs(
                    loader.project_root().identity(),
                    &candidate,
                    &independent,
                ));
                let snapshot = loader
                    .load_with_runtime_agreement(&candidate, &plan, agreement)
                    .unwrap();
                assert!(snapshot.has_runtime_schema_agreement());
                drop(snapshot);
            } else {
                let error = result.err().unwrap();
                let code = if case == "missing" {
                    "project.runtime-agreement.selected-owner-missing"
                } else {
                    "project.runtime-agreement.selected-owner-divergent"
                };
                assert!(
                    error
                        .diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str() == code)
                );
            }
            assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        }
    }

    #[derive(Default)]
    struct DataContentPlugin;

    impl Plugin for DataContentPlugin {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("test.project-content.data"),
                PluginCategory::Runtime,
            );
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
    }

    fn data_owner() -> ComponentSchemaOwnerSnapshot {
        ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("test.project-content.data").unwrap(),
            ComponentSchemaCatalog {
                components: [
                    ("test.content.Score", ComponentValueKind::U64),
                    ("test.content.Image", ComponentValueKind::AssetRef),
                ]
                .into_iter()
                .map(|(name, kind)| {
                    let mut field = ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::empty(),
                        kind,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
                    if kind == ComponentValueKind::AssetRef {
                        field = field.with_capability(ComponentCapability::AssetRef);
                    }
                    ComponentSchema::new(
                        ComponentTypeId::new(name),
                        name,
                        ComponentSchemaVersion::ONE,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([field])
                })
                .collect(),
                ..Default::default()
            },
            None,
        )
        .unwrap()
    }

    fn data_recipe(owner: &ComponentSchemaOwnerSnapshot) -> ProductRecipe {
        ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<DataContentPlugin>::data_for_default([owner.clone()]).unwrap(),
            )
            .unwrap()
    }

    #[test]
    fn escaped_scene_source_retains_actual_dependency_documents_and_exact_budget() {
        let project = LazyContentProject::new();
        fs::write(
            project.root.join("assets/unused.png.meta"),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    "3ae1ae89-6ba9-439c-8d7e-32f39fbcbb4a".parse().unwrap(),
                    AssetPath::new("unused.png").unwrap(),
                    AssetSourceKind::Image,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let owner = data_owner();
        let id: StableAssetId = "460bc37a-36e1-409e-9a39-0cc772810e0e".parse().unwrap();
        let product = AssetProductRef::new(id, ImportedProductId::PRIMARY).unwrap();
        let root = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("door").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("test.content.Image"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                nara_reflect::encode_asset_reference_value(&AssetRef::Product(product)),
            )
            .with_owner(owner.owner_id().clone()),
        )]);
        let dependency = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("retained-dependency").unwrap(),
        )]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            root.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("scenes/dependency.scene.json"),
            dependency.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("scenes/dependency.scene.json.meta"),
            nara_asset::AssetMeta::new(
                nara_asset::AssetMetaIdentity::new(
                    id,
                    AssetPath::new("dependency.scene.json").unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let (candidate, plan, loader) = project.inputs_with_recipe(data_recipe(&owner));
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert_eq!(snapshot.scene_dependencies().len(), 1);
        assert_eq!(snapshot.scene_dependencies()[0].document(), &dependency);
        assert_eq!(
            snapshot.scene_dependencies()[0].path().as_str(),
            "scenes/dependency.scene.json"
        );
        let raw = Arc::downgrade(&snapshot.inner.sources.scenes.dependencies[0].document);
        let effective = Arc::downgrade(&snapshot.inner.sources.scenes.dependencies[0].expanded);
        let source = snapshot.scene_source();
        let exact = loader
            .budget_snapshot()
            .high_water(ProjectContentBudgetKind::RetainedBytes);
        drop(snapshot);
        assert!(raw.upgrade().is_some());
        assert!(effective.upgrade().is_some());
        assert!(loader.budget_snapshot().active_reservations() > 0);
        drop(source);
        assert!(raw.upgrade().is_none());
        assert!(effective.upgrade().is_none());
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);

        for limit in [exact, exact - 1] {
            let limits = ProjectContentLimits::default()
                .with_limit(ProjectContentBudgetKind::RetainedBytes, limit)
                .unwrap();
            let loader = ProjectContentLoader::with_limits(loader.project_root(), limits).unwrap();
            let captured = loader.load(&candidate, &plan);
            if limit == exact {
                assert_eq!(
                    captured.as_ref().unwrap().scene_dependencies()[0].document(),
                    &dependency
                );
            } else {
                assert_eq!(
                    captured.as_ref().unwrap_err().kind(),
                    ProjectContentErrorKind::BudgetExceeded
                );
            }
            drop(captured);
            assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        }
    }

    #[test]
    fn selected_scene_metadata_uses_canonical_location_across_overlapping_readers() {
        for stable_native in [false, true] {
            for outer_declaration in [false, true] {
                let project = LazyContentProject::new();
                fs::rename(project.root.join("assets"), project.root.join("data")).unwrap();
                fs::rename(
                    project.root.join("scenes"),
                    project.root.join("data/scenes"),
                )
                .unwrap();
                let manifest_path = project.root.join("nara.toml");
                let mut manifest = fs::read_to_string(&manifest_path)
                    .unwrap()
                    .replace("assets = \"assets\"", "assets = \"data\"")
                    .replace("scenes = \"scenes\"", "scenes = \"data/scenes\"");
                if stable_native {
                    manifest = manifest.replace("'project_location'", "'stable_metadata'");
                }
                manifest.push_str("\n[content]\nscenes = ['b.scene.json']\n");
                fs::write(manifest_path, manifest).unwrap();
                let source_id: StableAssetId =
                    "3e851336-8076-448e-a6e4-1887ce18c00f".parse().unwrap();
                let startup_id: StableAssetId =
                    "f6213574-0d29-4ca3-9a18-e00435593684".parse().unwrap();
                for (alias, stable) in [
                    ("b.scene.json", source_id),
                    ("startup.scene.json", startup_id),
                ] {
                    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                        SceneEntityId::new(alias).unwrap(),
                    )]);
                    fs::write(
                        project.root.join(format!("data/scenes/{alias}")),
                        document.to_json_string().unwrap(),
                    )
                    .unwrap();
                    let declared = if outer_declaration {
                        format!("scenes/{alias}")
                    } else {
                        alias.to_owned()
                    };
                    fs::write(
                        project.root.join(format!("data/scenes/{alias}.meta")),
                        AssetMeta::new(
                            AssetMetaIdentity::new(
                                stable,
                                AssetPath::new(declared).unwrap(),
                                AssetSourceKind::Scene,
                            ),
                            (),
                        )
                        .to_json_string()
                        .unwrap(),
                    )
                    .unwrap();
                }
                fs::write(
                    project.root.join("data/unused.png.meta"),
                    AssetMeta::new(
                        AssetMetaIdentity::new(
                            "e3d8974d-38e2-4782-b2c4-7a1b5d6c0794".parse().unwrap(),
                            AssetPath::new("unused.png").unwrap(),
                            AssetSourceKind::Image,
                        ),
                        (),
                    )
                    .to_json_string()
                    .unwrap(),
                )
                .unwrap();
                let (candidate, plan, loader) = project.inputs();
                let startup = loader.load(&candidate, &plan).unwrap();
                assert_eq!(
                    startup.expanded_scene().entities[0].id.as_str(),
                    "startup.scene.json"
                );
                drop(startup);
                let product = AssetProductRef::new(source_id, ImportedProductId::PRIMARY).unwrap();
                for by_product in [false, true] {
                    let snapshot = if by_product {
                        loader
                            .load_scene_product(&candidate, &plan, product)
                            .unwrap()
                    } else {
                        loader
                            .load_scene(
                                &candidate,
                                &plan,
                                &nara_project::ProjectPath::new("b.scene.json").unwrap(),
                            )
                            .unwrap()
                    };
                    assert_eq!(snapshot.selected_scene_path().as_str(), "b.scene.json");
                    assert_eq!(
                        snapshot.expanded_scene().entities[0].id.as_str(),
                        "b.scene.json"
                    );
                    let metadata_observed = by_product || stable_native;
                    assert_eq!(
                        snapshot.selected_scene_product(),
                        metadata_observed.then_some(product)
                    );
                    assert_eq!(
                        matches!(
                            snapshot.selected_scene_source_key().origin(),
                            nara_scene::SourceDocumentOrigin::StableAsset { .. }
                        ),
                        stable_native
                    );
                    assert_eq!(snapshot.content_closure().members().iter().any(|member|
                        member.path().as_str() == "data/scenes/b.scene.json.meta"), metadata_observed);
                    drop(snapshot);
                    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
                }
                fs::write(
                    project.root.join("data/scenes/b.scene.json.meta"),
                    AssetMeta::new(
                        AssetMetaIdentity::new(
                            source_id,
                            AssetPath::new("wrong/b.scene.json").unwrap(),
                            AssetSourceKind::Scene,
                        ),
                        (),
                    )
                    .to_json_string()
                    .unwrap(),
                )
                .unwrap();
                let error = loader
                    .load_scene_product(&candidate, &plan, product)
                    .unwrap_err();
                assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
                assert_eq!(loader.budget_snapshot().active_reservations(), 0);
            }
        }
    }

    #[test]
    fn scene_data_dependency_uses_its_actual_reader_binding_without_kind_guessed_directory() {
        let project = LazyContentProject::new();
        let owner = data_owner();
        let id: StableAssetId = "850001f4-f7ae-4b6e-abba-bc0b2f786fec".parse().unwrap();
        let target = AssetRef::path("shared.scene.json").unwrap();
        let root = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("test.content.Image"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                nara_reflect::encode_asset_reference_value(&target),
            )
            .with_owner(owner.owner_id().clone()),
        )]);
        let dependency = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("actual-asset-directory-data").unwrap(),
        )]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            root.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("assets/shared.scene.json"),
            dependency.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("assets/shared.scene.json.meta"),
            nara_asset::AssetMeta::new(
                nara_asset::AssetMetaIdentity::new(
                    id,
                    AssetPath::new("shared.scene.json").unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("scenes/shared.scene.json"),
            b"wrong directory must not be read",
        )
        .unwrap();
        let (candidate, plan, loader) = project.inputs_with_recipe(data_recipe(&owner));
        let snapshot = loader.load(&candidate, &plan).unwrap();
        let captured = &snapshot.scene_dependencies()[0];
        assert_eq!(captured.path().as_str(), "assets/shared.scene.json");
        assert_eq!(captured.document(), &dependency);
        assert!(matches!(captured.source_key().origin(),
            nara_scene::SourceDocumentOrigin::ProjectLocation { path, .. }
                if path.as_str() == "assets/shared.scene.json"));
        assert!(
            snapshot
                .content_closure()
                .members()
                .iter()
                .any(
                    |member| member.kind() == ProjectContentMemberKind::AssetMetadata
                        && member.path().as_str() == "assets/shared.scene.json.meta"
                )
        );
        assert!(
            !snapshot
                .content_closure()
                .members()
                .iter()
                .any(|member| member.path().as_str() == "scenes/shared.scene.json")
        );
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn same_named_prefab_products_use_actual_readers_and_canonical_cache_keys() {
        let project = LazyContentProject::new();
        fs::write(
            project.root.join("assets/unused.png.meta"),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    "c20993f3-7a22-4c64-af02-ab7e69929bc5".parse().unwrap(),
                    AssetPath::new("unused.png").unwrap(),
                    AssetSourceKind::Image,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let products = [
            (
                "assets",
                "297933ba-1685-4dbd-bf87-a5b469a310b7",
                "asset-prefab",
            ),
            (
                "prefabs",
                "4922fab2-7b6d-4c7b-b2c5-4585a657d5f4",
                "prefab-prefab",
            ),
        ];
        let mut roots = Vec::new();
        for (directory, identity, local) in products {
            let stable_id: StableAssetId = identity.parse().unwrap();
            let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new(local).unwrap(),
            )]);
            fs::write(
                project.root.join(directory).join("shared.prefab.json"),
                prefab.to_json_string().unwrap(),
            )
            .unwrap();
            fs::write(
                project.root.join(directory).join("shared.prefab.json.meta"),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        stable_id,
                        AssetPath::new("shared.prefab.json").unwrap(),
                        AssetSourceKind::Prefab,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
            let mut anchor = SceneEntityRecord::new(SceneEntityId::new(local).unwrap());
            anchor.prefab = Some(PrefabInstance {
                source: AssetRef::Product(
                    AssetProductRef::new(stable_id, ImportedProductId::PRIMARY).unwrap(),
                ),
                overrides: None,
            });
            roots.push(anchor);
        }
        let document = SceneDocument::from_ordered_roots(roots);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        let (candidate, plan, loader) = project.inputs();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert_eq!(
            snapshot
                .prefabs()
                .iter()
                .map(|prefab| prefab.path().as_str())
                .collect::<Vec<_>>(),
            ["assets/shared.prefab.json", "prefabs/shared.prefab.json"]
        );
        assert!(
            snapshot
                .prefabs()
                .iter()
                .any(|prefab| prefab.document().roots[0].as_str() == "asset-prefab")
        );
        assert!(
            snapshot
                .prefabs()
                .iter()
                .any(|prefab| prefab.document().roots[0].as_str() == "prefab-prefab")
        );
        let rebound = snapshot
            .prepare_editor_startup_scene(
                snapshot.selected_scene_source_key(),
                snapshot.scene(),
                plan.schema_validation().bindings(),
            )
            .unwrap();
        assert_eq!(rebound.document(), snapshot.expanded_scene());
        drop((rebound, snapshot));
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn long_containing_identity_is_admitted_independently_of_compiler_work_limit() {
        let project = LazyContentProject::new();
        let (candidate, plan, original) = project.inputs();
        let compiler_limit = ByteLimit::new(4096).unwrap();
        let limits = ProjectContentLimits::default().with_prefab_expansion(
            nara_scene::PrefabExpansionLimits::default().with_working_bytes(compiler_limit),
        );
        let loader = ProjectContentLoader::with_limits(original.project_root(), limits).unwrap();
        let mut context = LoadContext::new(
            &loader,
            plan.schema_validation(),
            &plan.asset_domains,
            None,
            BTreeMap::new(),
            candidate.manifest_digest(),
            None,
            None,
            false,
        )
        .unwrap();
        let source = SourceDocumentKey::project_location(
            candidate.settings().project.source_universe,
            AssetSourcePath::new(format!("scenes/{}.scene.json", "x".repeat(8192))).unwrap(),
            SourceDocumentKind::Scene,
        );
        let compiled = context
            .compile_scene(
                &SceneDocument::default(),
                &source,
                candidate.settings(),
                &mut BTreeMap::new(),
                &mut 0,
            )
            .unwrap();
        assert_eq!(compiled.source_key(), &source);
        assert!(
            context.budget_value(ProjectContentBudgetKind::RetainedBytes)
                >= source.retained_bytes()
        );
        drop((compiled, context));
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn runtime_content_loads_only_selected_prefabs_and_effective_asset_facts() {
        let project = LazyContentProject::new();
        let (candidate, plan, loader) = project.inputs();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert_eq!(snapshot.prefabs().len(), 2);
        assert_eq!(snapshot.images().len(), 0);
        assert_eq!(
            loader
                .budget_snapshot()
                .high_water(ProjectContentBudgetKind::Files),
            3,
            "two occurrences share the actual P and R source reads"
        );
        let closure = snapshot.content_closure();
        let prefabs = closure
            .members()
            .iter()
            .filter(|member| member.kind() == ProjectContentMemberKind::Prefab)
            .map(|member| (member.path().as_str(), member.digest()))
            .collect::<Vec<_>>();
        assert_eq!(
            prefabs,
            [
                (
                    "prefabs/p.prefab.json",
                    project.digest("prefabs/p.prefab.json")
                ),
                (
                    "prefabs/r.prefab.json",
                    project.digest("prefabs/r.prefab.json")
                ),
            ]
        );
        let preserved = snapshot
            .prefabs()
            .iter()
            .find(|prefab| prefab.path().as_str() == "prefabs/p.prefab.json")
            .unwrap();
        let nested = preserved
            .document()
            .entities
            .iter()
            .find(|entity| entity.id.as_str() == "n")
            .unwrap();
        assert_eq!(
            nested.prefab.as_ref().unwrap().source,
            AssetRef::path("missing-q.prefab.json").unwrap()
        );
        assert!(nested.prefab.as_ref().unwrap().overrides.is_some());
        assert!(
            nested
                .components
                .contains_key(&ComponentTypeId::new("nara.sprite.Sprite"))
        );
        assert!(
            snapshot
                .expanded_scene()
                .entities
                .iter()
                .all(|entity| entity.components.is_empty())
        );
        assert_eq!(
            snapshot
                .expanded_scene_provenance()
                .iter()
                .filter(|(_, origin)| origin.local_entity_id().as_str() == "r")
                .count(),
            2
        );
        drop(closure);
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn runtime_content_checks_saved_receipts_against_actual_selected_sources() {
        let project = LazyContentProject::new();
        let (candidate, plan, loader) = project.inputs();
        for (path, expected_code) in [
            (
                "missing-q.prefab.json",
                "project.content.saved-prefab-outside-closure",
            ),
            (
                "r.prefab.json",
                "project.content.saved-prefab-source-changed",
            ),
        ] {
            let mut context = LoadContext::new(
                &loader,
                plan.schema_validation(),
                &plan.asset_domains,
                None,
                [(
                    nara_asset::AssetSourcePath::new(format!("prefabs/{path}")).unwrap(),
                    ContentDigest::of_bytes(b"saved bytes"),
                )]
                .into(),
                candidate.manifest_digest(),
                None,
                None,
                false,
            )
            .unwrap();
            let error = context.load(&candidate, None).unwrap_err();
            assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == expected_code)
            );
            drop(context);
            assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        }
    }

    #[test]
    fn runtime_content_lazy_decode_keeps_the_live_expansion_work_charged() {
        let project = LazyContentProject::new();
        let (candidate, plan, default_loader) = project.inputs();
        // Keep the compiler ceiling fixed below the source decoder's peak. The observed
        // boundary then measures the live working set during decoding, not unused capacity.
        let limits = ProjectContentLimits::default().with_prefab_expansion(
            nara_scene::PrefabExpansionLimits::default()
                .with_working_bytes(nara_core::ByteLimit::new(1024 * 1024).unwrap()),
        );
        let loader =
            ProjectContentLoader::with_limits(default_loader.project_root(), limits).unwrap();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        let required = loader
            .budget_snapshot()
            .high_water(ProjectContentBudgetKind::WorkBytes);
        drop(snapshot);
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);

        let exact_limits = limits
            .with_limit(ProjectContentBudgetKind::WorkBytes, required)
            .unwrap();
        let exact = ProjectContentLoader::with_limits(loader.project_root(), exact_limits).unwrap();
        let snapshot = exact.load(&candidate, &plan).unwrap();
        assert_eq!(
            exact
                .budget_snapshot()
                .high_water(ProjectContentBudgetKind::WorkBytes),
            required
        );
        drop(snapshot);
        assert_eq!(exact.budget_snapshot().active_reservations(), 0);

        let short_limits = limits
            .with_limit(ProjectContentBudgetKind::WorkBytes, required - 1)
            .unwrap();
        let short = ProjectContentLoader::with_limits(loader.project_root(), short_limits).unwrap();
        let error = short.load(&candidate, &plan).unwrap_err();
        assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
        assert_eq!(
            short
                .budget_snapshot()
                .high_water(ProjectContentBudgetKind::Files),
            3,
            "the complete partial expansion must remain charged while decoding R"
        );
        assert_eq!(short.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn runtime_content_resolved_alias_owns_exact_work_until_cache_or_retained_transfer() {
        use nara_asset::{AssetPathScope, AssetProductRef, AssetSourcePath, ImportedProductId};

        let project = LazyContentProject::new();
        let (candidate, plan, default_loader) = project.inputs();
        let root = nara_project::ProjectPath::new("resources/selected/images").unwrap();
        let scope = AssetPathScope::under(AssetSourcePath::new(root.as_str()).unwrap());
        let alias =
            AssetPath::new(format!("textures/{}/image.png", "long-name".repeat(8))).unwrap();
        let stable_id = StableAssetId::parse_str("f7f46abe-83ac-419d-8361-bd61ee4c2d47").unwrap();
        let product = AssetProductRef::new(stable_id, ImportedProductId::PRIMARY).unwrap();
        let path_work = super::string_retained_bytes(alias.as_str()).unwrap();
        fs::create_dir_all(
            project
                .root
                .join(root.as_str())
                .join("textures")
                .join("long-name".repeat(8)),
        )
        .unwrap();
        fs::write(
            project.root.join(root.as_str()).join(alias.meta_path()),
            AssetMeta::new(
                AssetMetaIdentity::new(stable_id, alias.clone(), AssetSourceKind::Image),
                nara_image::ImageImportSettings::default(),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let ceiling = 4 * 1024 * 1024;
        let limits = ProjectContentLimits::default()
            .with_limit(ProjectContentBudgetKind::WorkBytes, ceiling)
            .unwrap();
        let loader =
            ProjectContentLoader::with_limits(default_loader.project_root(), limits).unwrap();

        for reference in [
            AssetRef::Path(alias.clone()),
            AssetRef::StableId(stable_id),
            AssetRef::Product(product),
        ] {
            for remaining in [path_work - 1, path_work] {
                let mut context = LoadContext::new(
                    &loader,
                    plan.schema_validation(),
                    &plan.asset_domains,
                    None,
                    Default::default(),
                    candidate.manifest_digest(),
                    None,
                    None,
                    false,
                )
                .unwrap();
                context.asset_scope = scope.clone();
                context.prefab_scope =
                    super::source_scope(&candidate.settings().paths.prefabs).unwrap();
                context.catalogue_roots = Some(vec![
                    root.clone(),
                    candidate.settings().paths.prefabs.clone(),
                ]);
                context.ensure_reference_catalogue().unwrap();
                // The catalogue and scope are already admitted. Leave only the selected
                // allowance for an alias much longer than its StableId/Product selector.
                let baseline = ceiling - remaining;
                context
                    .set_budget(ProjectContentBudgetKind::WorkBytes, baseline)
                    .unwrap();
                let retained_before = context.budget_value(ProjectContentBudgetKind::RetainedBytes);
                let result =
                    context.resolve_reference_path(&reference, &AssetSourceKind::Image, &root);
                if remaining < path_work {
                    assert_eq!(
                        result.unwrap_err().kind(),
                        ProjectContentErrorKind::BudgetExceeded
                    );
                    assert_eq!(
                        context.budget_value(ProjectContentBudgetKind::WorkBytes),
                        baseline
                    );
                } else {
                    let (resolved, reservation) = result.unwrap();
                    assert_eq!(resolved, alias);
                    assert_eq!(reservation.reserved, path_work);
                    assert_eq!(
                        context.budget_value(ProjectContentBudgetKind::WorkBytes),
                        ceiling
                    );
                    // Unrelated queued work may retire while this alias is still live.
                    context.subtract_budget(ProjectContentBudgetKind::WorkBytes, 1);
                    drop(resolved);
                    context
                        .commit_work_as_retained(reservation, path_work)
                        .unwrap();
                    assert_eq!(
                        context.budget_value(ProjectContentBudgetKind::WorkBytes),
                        baseline - 1
                    );
                    assert_eq!(
                        context.budget_value(ProjectContentBudgetKind::RetainedBytes),
                        retained_before + path_work
                    );
                }
                drop(context);
                assert_eq!(loader.budget_snapshot().active_reservations(), 0);
            }
        }
    }

    #[test]
    fn runtime_content_rejects_invalid_active_native_components() {
        let project = LazyContentProject::new();
        let invalid = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("r").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Bool(true)),
        )]);
        fs::write(
            project.root.join("prefabs/r.prefab.json"),
            invalid.to_json_string().unwrap(),
        )
        .unwrap();
        let (candidate, plan, loader) = project.inputs();
        let error = loader.load(&candidate, &plan).unwrap_err();
        assert!(error.diagnostics().has_errors());
        assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    }

    #[test]
    fn runtime_content_defers_source_payload_admission_until_complete_repair_is_selected() {
        for repair in ["replace", "suppress", "inherit"] {
            let project = LazyContentProject::new();
            let id = |name| SceneEntityId::new(name).unwrap();
            let component = ComponentTypeId::new("nara.scene.Name");
            let source = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("n"))
                .with_component(
                    component.clone(),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::Bool(true),
                    ),
                )]);
            fs::write(
                project.root.join("prefabs/p.prefab.json"),
                source.to_json_string().unwrap(),
            )
            .unwrap();
            let mut overlay = PrefabOverlay::new(SourceDocumentKey::project_location(
                "39f9d09d-63e3-5fc4-8e3f-71e84893cb8d".parse().unwrap(),
                nara_asset::AssetSourcePath::new("prefabs/p.prefab.json").unwrap(),
                SourceDocumentKind::Prefab,
            ));
            match repair {
                "replace" => overlay.components.push(PrefabComponentIntent::Set {
                    mode: nara_scene::PrefabComponentSetMode::Replace,
                    target: PrefabEntityTarget::source(id("n")),
                    component: component.clone(),
                    record: SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String("Repaired".into()),
                    ),
                    bindings: Vec::new(),
                }),
                "suppress" => overlay.components.push(PrefabComponentIntent::Suppress {
                    target: PrefabEntityTarget::source(id("n")),
                    component: component.clone(),
                }),
                _ => {}
            }
            let mut anchor = SceneEntityRecord::new(id("host"));
            anchor.prefab = Some(PrefabInstance {
                source: AssetRef::path("p.prefab.json").unwrap(),
                overrides: Some(overlay),
            });
            fs::write(
                project.root.join("scenes/startup.scene.json"),
                SceneDocument::from_ordered_roots([anchor])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            let (candidate, plan, loader) = project.inputs();
            let result = loader.load(&candidate, &plan);
            if repair == "inherit" {
                assert!(
                    result.is_err(),
                    "retaining the invalid value must still refuse native publication"
                );
            } else {
                let snapshot = result.unwrap();
                assert!(!snapshot.source_upgrade_required());
                assert_eq!(snapshot.prefabs().len(), 1);
                assert_eq!(
                    snapshot.prefabs()[0].document().entities[0].components[&component].value,
                    ComponentValue::Bool(true),
                    "composition cannot rewrite the captured source",
                );
                let node = snapshot
                    .expanded_scene()
                    .entities
                    .iter()
                    .find(|entity| entity.id != id("host"))
                    .unwrap();
                assert_eq!(
                    node.components.contains_key(&component),
                    repair == "replace"
                );
                if repair == "replace" {
                    assert_eq!(
                        node.components[&component].value,
                        ComponentValue::String("Repaired".into())
                    );
                }
                drop(snapshot);
            }
            assert_eq!(loader.budget_snapshot().active_reservations(), 0);
        }
    }

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct LazyContentProject {
        root: PathBuf,
        temporary_parent: PathBuf,
    }

    impl LazyContentProject {
        fn new() -> Self {
            let temporary_parent = std::env::temp_dir().canonicalize().unwrap();
            let root = temporary_parent.join(format!(
                "nara_lazy_project_content_{}_{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&root).unwrap();
            let project = Self {
                root,
                temporary_parent,
            };
            for directory in ["assets", "prefabs", "scenes"] {
                fs::create_dir(project.root.join(directory)).unwrap();
            }
            fs::write(
                project.root.join("nara.toml"),
                r#"schema_version = 1
[project]
source_universe = '39f9d09d-63e3-5fc4-8e3f-71e84893cb8d'
native_document_identity = 'project_location'
name = "Lazy content regression"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[capabilities]
requested = ["runtime-2d"]
"#,
            )
            .unwrap();
            let id = |name| SceneEntityId::new(name).unwrap();
            let key = |name| {
                SourceDocumentKey::project_location(
                    "39f9d09d-63e3-5fc4-8e3f-71e84893cb8d".parse().unwrap(),
                    nara_asset::AssetSourcePath::new(format!("prefabs/{name}.prefab.json"))
                        .unwrap(),
                    SourceDocumentKind::Prefab,
                )
            };
            let mut discarded = PrefabAddition::new(id("unavailable"));
            discarded.components.insert(
                ComponentTypeId::new("test.Unavailable"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
            );
            let mut obsolete = PrefabOverlay::new(key("missing-q"));
            obsolete.additions.push(discarded);
            obsolete.placements.push(PrefabPlacement {
                target: PrefabEntityTarget::addition(id("unavailable")),
                parent: PrefabPlacementParent::Root,
                slot: PrefabPlacementSlot::Last,
            });
            let mut nested = SceneEntityRecord::new(id("n")).with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        (
                            "size",
                            ComponentValue::map([
                                ("x", ComponentValue::f64(1.0).unwrap()),
                                ("y", ComponentValue::f64(1.0).unwrap()),
                            ]),
                        ),
                        (
                            "material",
                            ComponentValue::map([
                                (
                                    "image",
                                    ComponentValue::map([
                                        ("kind", ComponentValue::String("stable_id".into())),
                                        (
                                            "value",
                                            ComponentValue::String(
                                                "706fd198-2412-4ccb-a398-f9f826fbe208".into(),
                                            ),
                                        ),
                                    ]),
                                ),
                                (
                                    "tint",
                                    ComponentValue::map([
                                        ("r", ComponentValue::f64(1.0).unwrap()),
                                        ("g", ComponentValue::f64(1.0).unwrap()),
                                        ("b", ComponentValue::f64(1.0).unwrap()),
                                        ("a", ComponentValue::f64(1.0).unwrap()),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                ),
            );
            nested.prefab = Some(PrefabInstance {
                source: AssetRef::path("missing-q.prefab.json").unwrap(),
                overrides: Some(obsolete),
            });
            let p = PrefabDocument::from_ordered_roots([nested]);
            let r = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("r"))]);
            let mut overlay = PrefabOverlay::new(key("p"));
            overlay.instances.push(PrefabInstanceClaim {
                target: PrefabEntityTarget::source(id("n")),
                value: Some(PrefabInstance {
                    source: AssetRef::path("r.prefab.json").unwrap(),
                    overrides: None,
                }),
            });
            overlay.components.push(PrefabComponentIntent::Suppress {
                target: PrefabEntityTarget::source(id("n")),
                component: ComponentTypeId::new("nara.sprite.Sprite"),
            });
            let scene = SceneDocument::from_ordered_roots(["outer-a", "outer-b"].map(|name| {
                let mut anchor = SceneEntityRecord::new(id(name));
                anchor.prefab = Some(PrefabInstance {
                    source: AssetRef::path("p.prefab.json").unwrap(),
                    overrides: Some(overlay.clone()),
                });
                anchor
            }));
            for (path, bytes) in [
                ("scenes/startup.scene.json", scene.to_json_string().unwrap()),
                ("prefabs/p.prefab.json", p.to_json_string().unwrap()),
                ("prefabs/r.prefab.json", r.to_json_string().unwrap()),
            ] {
                fs::write(project.root.join(path), bytes).unwrap();
            }
            // Any premature stable-ID catalogue preparation encounters this invalid metadata.
            fs::write(
                project.root.join("assets/unused.png.meta"),
                b"invalid metadata",
            )
            .unwrap();
            project
        }

        fn inputs(&self) -> (ProjectSettingsCandidate, RuntimePlan, ProjectContentLoader) {
            self.inputs_with_recipe(ProductRecipe::new())
        }

        fn inputs_with_recipe(
            &self,
            recipe: ProductRecipe,
        ) -> (ProjectSettingsCandidate, RuntimePlan, ProjectContentLoader) {
            #[cfg(windows)]
            let file = {
                use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
                OpenOptions::new()
                    .read(true)
                    .share_mode(0x1 | 0x2 | 0x4)
                    .custom_flags(0x0200_0000 | 0x0020_0000)
                    .open(&self.root)
                    .unwrap()
            };
            #[cfg(not(windows))]
            let file = fs::File::open(&self.root).unwrap();
            let root = DirectoryCapability::from_host_handle(
                file,
                HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
            )
            .unwrap();
            let manifest = root
                .open_file(&RelativePath::new("nara.toml").unwrap())
                .unwrap();
            let candidate = ingest_project_manifest(&manifest, None).unwrap();
            let plan = resolve_product_recipe(&candidate, recipe).unwrap();
            (candidate, plan, ProjectContentLoader::new(root).unwrap())
        }

        fn digest(&self, path: &str) -> ContentDigest {
            ContentDigest::of_bytes(&fs::read(self.root.join(path)).unwrap())
        }
    }

    impl Drop for LazyContentProject {
        fn drop(&mut self) {
            let relative = self.root.strip_prefix(&self.temporary_parent).unwrap();
            assert_eq!(relative.components().count(), 1);
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
