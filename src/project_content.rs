//! Authorized, World-independent Scene content loading.

mod budget;
mod catalogue;
mod closure;
mod delivery;
mod domains;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetMetaCandidate, AssetPath, AssetRecord, AssetRef, AssetSourceKind, ProjectAssetDatabase,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_fs::{ContentDigest, DirectoryCapability, FsError, FsOperation, RelativePath};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageImportError, ImageImportMemoryPlan,
    ImageImportSettings, ImageImporter, ImageImporterCreateError,
};
use nara_reflect::__private::{
    ComponentRegistrySnapshotWitness, component_registry_snapshot_witness,
    component_registry_snapshot_witness_matches,
};
use nara_reflect::{
    ComponentRegistrySnapshot, DeclaredAssetReferenceError, SchemaCompositionFingerprint,
    collect_declared_asset_references, collect_declared_field_asset_references,
};
use nara_scene::{
    ExpandedSceneCandidate, PrefabDocument, PrefabDocumentCandidate, PrefabSourceResolver,
    SceneDocument, SceneDocumentCandidate, SceneEntityProvenanceMap, SceneEntityRecord,
    SceneFormatError, ScenePatchDocument, ScenePatchOperation,
};
use nara_tilemap::{
    TileSetSource, TileSetSourceFileLimits, TilemapMembershipError,
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
pub use domains::{
    ProjectAssetCollection, ProjectAssetDomain, ProjectAssetLimits, ProjectAssetSource,
    ProjectDomainAssets,
};

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
    path: AssetPath,
    document: Arc<PrefabDocument>,
}

impl ProjectPrefabContent {
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub fn document(&self) -> &PrefabDocument {
        &self.document
    }
}

#[derive(Debug)]
pub struct ProjectImageContent {
    image: ImageAsset,
}

#[derive(Debug)]
pub struct ProjectTileSetContent {
    stable_id: nara_asset::StableAssetId,
    path: AssetPath,
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
    pub const fn path(&self) -> &AssetPath {
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
    pub const fn path(&self) -> &AssetPath {
        self.image.source().path()
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
    schema_authority: ComponentRegistrySnapshotWitness,
    domain_authority: ProjectAssetDomains,
    content_closure: ProjectContentClosureReceipt,
    source_upgrade_required: bool,
    scene: Arc<SceneDocument>,
    expanded_scene: Arc<ExpandedSceneCandidate>,
    prefabs: Box<[ProjectPrefabContent]>,
    images: Box<[ProjectImageContent]>,
    tilesets: Box<[ProjectTileSetContent]>,
    domain_assets: ProjectDomainAssets,
    assets: Arc<ProjectContentAssets>,
    _generation_lease: ProjectContentGenerationLease,
    _runtime_lease: ProjectContentLease,
}

// Pixel consumers retain the byte reservation and generation claim without retaining the
// snapshot, its Images, or any decoded Scene candidate back through this owner.
struct ProjectImageRetention {
    _lease: ProjectContentLease,
    _generation: ProjectContentGenerationLease,
}

/// Keeps a materializer's admitted identity context and its reservation together.
pub(crate) struct ProjectContentAssets {
    database: ProjectAssetDatabase,
    _lease: ProjectContentLease,
}

impl ProjectContentAssets {
    pub(crate) fn database(&self) -> &ProjectAssetDatabase {
        &self.database
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
    pub(crate) fn shares_schema_snapshot(&self, snapshot: &ComponentRegistrySnapshot) -> bool {
        component_registry_snapshot_witness_matches(&self.inner.schema_authority, snapshot)
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

    #[must_use]
    pub fn source_upgrade_required(&self) -> bool {
        self.inner.source_upgrade_required
    }

    #[must_use]
    pub fn scene(&self) -> &SceneDocument {
        &self.inner.scene
    }

    #[must_use]
    pub fn expanded_scene(&self) -> &SceneDocument {
        self.inner.expanded_scene.document()
    }

    /// Returns the structured source identity paired with the expanded selected document.
    #[must_use]
    pub fn expanded_scene_provenance(&self) -> &SceneEntityProvenanceMap {
        self.inner.expanded_scene.provenance()
    }

    #[must_use]
    pub fn prefabs(&self) -> &[ProjectPrefabContent] {
        &self.inner.prefabs
    }

    pub(crate) fn resolved_asset_path(&self, reference: &AssetRef) -> Option<&AssetPath> {
        self.inner
            .assets
            .database()
            .resolve_ref(reference)
            .ok()
            .map(AssetRecord::path)
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
        self.inner.assets.database()
    }

    pub(crate) fn expanded_candidate(&self) -> &nara_scene::ExpandedSceneCandidate {
        &self.inner.expanded_scene
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
            Arc::clone(&self.inner.expanded_scene),
            Arc::clone(&self.inner.assets),
            self.inner._runtime_lease.clone(),
            self.inner._generation_lease.clone(),
            startup_scene_retained_bytes(
                self.inner.expanded_scene.document(),
                self.inner.expanded_scene.provenance(),
            )
            .expect("an admitted Scene already has a valid retained-byte plan"),
        )
    }

    pub(crate) fn prepare_editor_startup_scene(
        &self,
        document: &SceneDocument,
        registry: &nara_reflect::ComponentRegistry,
    ) -> Result<SceneSource, ProjectContentError> {
        let resolver = SnapshotPrefabResolver {
            prefabs: self.prefabs(),
            database: self.inner.assets.database(),
        };
        let expansion = document.expand_prefabs_with_options_and_asset_database(
            registry,
            &resolver,
            nara_scene::PrefabExpansionOptions::default(),
            self.inner.assets.database(),
        );
        let nara_scene::PrefabExpansionReport {
            candidate,
            diagnostics,
        } = expansion;
        let expanded_candidate = candidate.ok_or_else(|| {
            map_scene_diagnostics(ProjectContentErrorKind::PrefabExpansion, diagnostics)
        })?;
        let expanded = expanded_candidate.document();
        let provenance = expanded_candidate.provenance();
        let diagnostics =
            expanded.validate_with_asset_database(registry, self.inner.assets.database());
        if diagnostics.has_errors() {
            return Err(map_scene_diagnostics(
                ProjectContentErrorKind::ScenePublication,
                diagnostics,
            ));
        }
        let retained_bytes = editor_startup_scene_retained_bytes(&expanded, &provenance)?;
        let lease = self
            .inner
            ._runtime_lease
            .reserve_retained(retained_bytes)
            .map_err(ProjectContentError::budget)?;
        Ok(SceneSource::from_project_content(
            Arc::new(expanded_candidate),
            Arc::clone(&self.inner.assets),
            lease,
            self.inner._generation_lease.clone(),
            retained_bytes,
        ))
    }

    #[cfg(feature = "presentation-2d")]
    pub(crate) fn sync_editor_preview_scene(
        &self,
        session: &mut nara_scene::SceneAuthoringSession,
        world: &mut nara_ecs::World,
        registry: &nara_reflect::ComponentRegistry,
    ) -> nara_scene::SceneAuthoringSyncReport {
        let resolver = SnapshotPrefabResolver {
            prefabs: self.prefabs(),
            database: self.inner.assets.database(),
        };
        session
            .sync_world_with_prefab_resolver_and_asset_database(
                world,
                registry,
                &resolver,
                self.inner.assets.database(),
            )
            .expect("Editor preview materializers always retain native authority")
    }
}

struct SnapshotPrefabResolver<'a> {
    prefabs: &'a [ProjectPrefabContent],
    database: &'a ProjectAssetDatabase,
}

impl PrefabSourceResolver for SnapshotPrefabResolver<'_> {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        let path = match source {
            AssetRef::Path(path) => path,
            AssetRef::StableId(_) => self.database.resolve_ref(source).ok()?.path(),
        };
        self.prefabs
            .iter()
            .find(|prefab| prefab.path() == path)
            .map(ProjectPrefabContent::document)
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
    pub fn asset_domain(diagnostic: Diagnostic) -> Self {
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

    fn legacy_placement_shape(
        code: &'static str,
        summary: &'static str,
        key: &'static str,
        value: &str,
        entity_index: Option<usize>,
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
        let diagnostic = match entity_index {
            Some(entity_index) => attach_field(
                diagnostic,
                DiagnosticField::public_u64(
                    field_key("entity_index"),
                    u64::try_from(entity_index).unwrap_or(u64::MAX),
                ),
            ),
            None => diagnostic,
        };
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        Self::with_report(ProjectContentErrorKind::SceneFormat, diagnostics)
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
        self.load_inner(candidate, plan, None, None, None, None)
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
        self.load_inner(candidate, plan, Some(scene), None, None, None)
    }

    pub(crate) fn load_with_runtime_agreement(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        agreement: PreparedRuntimeSchemaAgreement,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(candidate, plan, None, None, None, Some(agreement))
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
            None,
            Some(agreement),
        )
    }

    pub(crate) fn load_with_expected_prefab_digest_and_runtime_agreement(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        path: AssetPath,
        digest: ContentDigest,
        agreement: PreparedRuntimeSchemaAgreement,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.load_inner(
            candidate,
            plan,
            None,
            None,
            Some(ExpectedPrefabSource {
                path,
                digest,
                observed: false,
            }),
            Some(agreement),
        )
    }

    fn load_inner(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
        selected_scene: Option<&nara_project::ProjectPath>,
        expected_startup_scene_digest: Option<ContentDigest>,
        expected_prefab: Option<ExpectedPrefabSource>,
        runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.validate_inputs(candidate, plan)?;
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
            expected_prefab,
            candidate.manifest_digest(),
            runtime_agreement,
        )?;
        context.load(candidate, selected_scene)
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
        let registry = plan.schema_validation().registry();
        let snapshot_fingerprint = plan
            .schema_validation()
            .snapshot()
            .schema_composition_fingerprint()
            .map_err(|_| {
                ProjectContentError::single(
                    ProjectContentErrorKind::SchemaMismatch,
                    "project.content.schema-unavailable",
                    "Project schema validation authority is unavailable",
                )
            })?;
        if !registry.shares_snapshot(plan.schema_validation().snapshot())
            || snapshot_fingerprint != plan.schema_validation().composition_fingerprint()
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
}

struct LoadContext<'a> {
    loader: &'a ProjectContentLoader,
    validation: &'a SchemaValidationInput,
    domains: &'a ProjectAssetDomains,
    budget: Option<BudgetTicket>,
    source_upgrade_required: bool,
    expected_startup_scene_digest: Option<ContentDigest>,
    expected_prefab: Option<ExpectedPrefabSource>,
    runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
    content_closure: Option<ProjectContentClosureBuilder>,
    content_closure_work_bytes: usize,
    asset_database: ProjectAssetDatabase,
    asset_database_work_bytes: usize,
    catalogue: Option<ReferenceCatalogue>,
    catalogue_roots: Option<[nara_project::ProjectPath; 2]>,
}

struct ExpectedPrefabSource {
    path: AssetPath,
    digest: ContentDigest,
    observed: bool,
}

#[derive(Debug, Clone, Copy)]
struct WorkReservation {
    previous: usize,
    reserved: usize,
}

impl<'a> LoadContext<'a> {
    fn new(
        loader: &'a ProjectContentLoader,
        validation: &'a SchemaValidationInput,
        domains: &'a ProjectAssetDomains,
        expected_startup_scene_digest: Option<ContentDigest>,
        expected_prefab: Option<ExpectedPrefabSource>,
        manifest_digest: ContentDigest,
        runtime_agreement: Option<PreparedRuntimeSchemaAgreement>,
    ) -> Result<Self, ProjectContentError> {
        let content_closure_work_bytes = content_closure_entry_work_bytes("nara.toml".len())?;
        let mut budget = loader.budget_host.reserve();
        budget
            .add(
                ProjectContentBudgetKind::WorkBytes,
                content_closure_work_bytes,
            )
            .map_err(ProjectContentError::budget)?;
        Ok(Self {
            loader,
            validation,
            domains,
            budget: Some(budget),
            source_upgrade_required: false,
            expected_startup_scene_digest,
            expected_prefab,
            runtime_agreement,
            content_closure: Some(ProjectContentClosureBuilder::new(manifest_digest)?),
            content_closure_work_bytes,
            asset_database: ProjectAssetDatabase::default(),
            asset_database_work_bytes: 0,
            catalogue: None,
            catalogue_roots: None,
        })
    }

    fn load(
        &mut self,
        candidate: &ProjectSettingsCandidate,
        selected_scene: Option<&nara_project::ProjectPath>,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.add_budget(ProjectContentBudgetKind::OpenHandles, 1)?;
        let settings = candidate.settings();
        self.catalogue_roots = Some([
            settings.paths.assets.clone(),
            settings.paths.prefabs.clone(),
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
        let scene_path = selected_scene
            .or(settings.startup.default_scene.as_ref())
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
        let scene = self.decode_scene(scene_path.as_str(), &scene_bytes)?;

        let loaded_prefabs = self.load_prefab_closure(&prefabs, &settings.paths.prefabs, &scene)?;
        for path in loaded_prefabs.keys() {
            self.admit_prefab_metadata(&prefabs, &settings.paths.prefabs, path)?;
        }
        if let Some(expected) = &self.expected_prefab
            && !expected.observed
        {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.saved-prefab-outside-closure",
                "Saved Prefab is no longer a member of the startup content closure",
                "prefab",
                expected.path.as_str(),
            ));
        }
        let expansion_work = prefab_expansion_work_plan(self.loader.limits.prefab_expansion())?;
        let work_before = self.reserve_work(expansion_work)?;
        let resolver = LoadedPrefabResolver::new(&loaded_prefabs, &self.asset_database);
        let options = nara_scene::PrefabExpansionOptions::default()
            .with_limits(self.loader.limits.prefab_expansion());
        let expansion = if self.catalogue.is_some() {
            scene.expand_prefabs_with_options_and_asset_database(
                self.validation.registry(),
                &resolver,
                options,
                &self.asset_database,
            )
        } else {
            scene.expand_prefabs_with_options(self.validation.registry(), &resolver, options)
        };
        let nara_scene::PrefabExpansionReport {
            candidate: expanded_candidate,
            diagnostics,
        } = expansion;
        let expanded_candidate = expanded_candidate.ok_or_else(|| {
            map_scene_diagnostics(ProjectContentErrorKind::PrefabExpansion, diagnostics)
        })?;
        let expanded = expanded_candidate.document();
        let provenance = expanded_candidate.provenance();
        let expanded_retained = startup_scene_retained_bytes(&expanded, &provenance)?;
        self.commit_work_as_retained(work_before, expanded_retained)?;

        if let Some(agreement) = self.runtime_agreement.take() {
            let (path, digest) = agreement.revalidate_and_into_closure_member(&self.loader.root)?;
            self.record_project_member(
                ProjectContentMemberKind::AuthoringSchemaBundle,
                path,
                digest,
            )?;
        }

        let sources = self.collect_source_candidates(expanded, &assets)?;
        let domain_collections =
            self.load_domains(&assets, &settings.paths.assets, sources.domains)?;
        let (tilesets, tileset_work) =
            self.load_tilesets(&assets, &settings.paths.assets, sources.tilesets)?;
        let mut image_paths = sources.images;
        for tileset in &tilesets {
            if image_paths.insert(tileset.image_path.clone()) {
                self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
            }
        }
        let images = self.load_images(&assets, &settings.paths.assets, image_paths)?;
        self.validate_tileset_image_extents(&tilesets, &images)?;
        self.validate_tilemap_membership(&expanded, &provenance, &tilesets)?;
        self.commit_tileset_identities(&settings.paths.assets, &tilesets)?;
        let diagnostics =
            expanded.validate_with_asset_database(self.validation.registry(), &self.asset_database);
        if diagnostics.has_errors() {
            return Err(map_scene_diagnostics(
                ProjectContentErrorKind::ScenePublication,
                diagnostics,
            ));
        }
        self.commit_work_as_retained(tileset_work, tileset_work.reserved)?;
        let database = self.retain_loaded_asset_identities(
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

        let snapshot_overhead =
            snapshot_retained_overhead(&loaded_prefabs, images.as_slice(), tilesets.as_slice())?;
        let content_closure_retained = content_closure_retained_bytes(&content_closure)?;
        let retained_bytes = self
            .budget_value(ProjectContentBudgetKind::RetainedBytes)
            .checked_add(snapshot_overhead)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
        self.set_budget(ProjectContentBudgetKind::RetainedBytes, retained_bytes)?;

        let scene = Arc::new(scene);
        let expanded_scene = Arc::new(expanded_candidate);
        let mut prefab_values = Vec::new();
        prefab_values
            .try_reserve_exact(loaded_prefabs.len())
            .map_err(|_| allocation_failed("project.content.prefab-list-allocation-failed"))?;
        prefab_values.extend(
            loaded_prefabs
                .into_iter()
                .map(|(path, document)| ProjectPrefabContent { path, document }),
        );
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
        let assets = Arc::new(ProjectContentAssets {
            database,
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
                schema_authority: component_registry_snapshot_witness(self.validation.snapshot()),
                domain_authority: self.domains.clone(),
                content_closure,
                source_upgrade_required: self.source_upgrade_required,
                scene,
                expanded_scene,
                prefabs,
                images,
                tilesets,
                domain_assets: ProjectDomainAssets::new(
                    domain_collections,
                    runtime_lease.clone(),
                    generation_lease.clone(),
                ),
                assets,
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
            RELATIVE_OPEN_HANDLE_PEAK,
        )?;
        match self.loader.root.open_directory(&relative) {
            Ok(directory) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::OpenHandles,
                    RELATIVE_OPEN_HANDLE_PEAK - RETAINED_DIRECTORY_HANDLES,
                );
                Ok(directory)
            }
            Err(error) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::OpenHandles,
                    RELATIVE_OPEN_HANDLE_PEAK,
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
        self.read_file_inner(directory, logical_path, encoded_ceiling, None)
    }

    fn read_selected_scene_file(
        &mut self,
        directory: &DirectoryCapability,
        logical_path: &str,
        encoded_ceiling: usize,
        is_startup: bool,
    ) -> Result<(Vec<u8>, ContentDigest), ProjectContentError> {
        self.read_file_inner(directory, logical_path, encoded_ceiling, Some(is_startup))
    }

    fn read_file_inner(
        &mut self,
        directory: &DirectoryCapability,
        logical_path: &str,
        encoded_ceiling: usize,
        selected_scene: Option<bool>,
    ) -> Result<(Vec<u8>, ContentDigest), ProjectContentError> {
        let relative = self.observe_path(logical_path)?;
        self.add_budget(ProjectContentBudgetKind::Files, 1)?;
        self.add_budget(
            ProjectContentBudgetKind::OpenHandles,
            RELATIVE_OPEN_HANDLE_PEAK,
        )?;
        let file = match directory.open_file(&relative) {
            Ok(file) => file,
            Err(error) => {
                self.subtract_budget(
                    ProjectContentBudgetKind::OpenHandles,
                    RELATIVE_OPEN_HANDLE_PEAK,
                );
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
        self.subtract_budget(
            ProjectContentBudgetKind::OpenHandles,
            RELATIVE_OPEN_HANDLE_PEAK,
        );
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

    fn load_prefab_closure(
        &mut self,
        prefabs: &DirectoryCapability,
        prefab_root: &nara_project::ProjectPath,
        scene: &SceneDocument,
    ) -> Result<BTreeMap<AssetPath, Arc<PrefabDocument>>, ProjectContentError> {
        let mut queue = VecDeque::new();
        let mut discovered = BTreeSet::new();
        for entity in &scene.entities {
            self.enqueue_prefab_references(entity, 1, &mut queue, &mut discovered)?;
        }
        let mut loaded = BTreeMap::new();
        while let Some((path, depth)) = queue.pop_front() {
            self.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            self.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
            let loaded_result = self.load_prefab(prefabs, prefab_root, &path);
            self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            let document = loaded_result?;
            for entity in &document.entities {
                self.enqueue_prefab_references(
                    entity,
                    depth
                        .checked_add(1)
                        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::DirectoryDepth))?,
                    &mut queue,
                    &mut discovered,
                )?;
            }
            loaded.insert(path, Arc::new(document));
        }
        Ok(loaded)
    }

    fn load_prefab(
        &mut self,
        prefabs: &DirectoryCapability,
        prefab_root: &nara_project::ProjectPath,
        path: &AssetPath,
    ) -> Result<PrefabDocument, ProjectContentError> {
        let (bytes, digest) = self.read_file(
            prefabs,
            path.as_str(),
            self.loader.limits.scene_file().encoded_bytes().get(),
        )?;
        if let Some(expected) = self.expected_prefab.as_mut()
            && expected.path == *path
        {
            expected.observed = true;
            if expected.digest != digest {
                return Err(ProjectContentError::with_identifier(
                    ProjectContentErrorKind::HostAuthorityRejected,
                    "project.content.saved-prefab-source-changed",
                    "Saved Prefab bytes do not match the accepted persistence receipt",
                    "prefab",
                    path.as_str(),
                ));
            }
        }
        self.record_member(
            ProjectContentMemberKind::Prefab,
            prefab_root,
            path.as_str(),
            digest,
        )?;
        self.decode_prefab(path.as_str(), &bytes)
    }

    fn decode_scene(
        &mut self,
        path: &str,
        bytes: &[u8],
    ) -> Result<SceneDocument, ProjectContentError> {
        let work_before =
            self.reserve_work(scene_decode_work_plan(self.loader.limits.scene_file())?)?;
        let canonical = decode_scene_candidate(path, bytes, self.loader.limits.scene_file())?
            .canonicalize(self.validation.registry())
            .map_err(map_scene_publication_error)?;
        self.prepare_component_asset_references(&canonical.document().entities)?;
        let published = if self.catalogue.is_some() {
            canonical.publish_with_asset_database(self.validation.registry(), &self.asset_database)
        } else {
            canonical.publish(self.validation.registry())
        }
        .map_err(map_scene_publication_error)?;
        self.source_upgrade_required |= published.source_upgrade_required();
        let document = published.into_document();
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
        let canonical = decode_prefab_candidate(path, bytes, self.loader.limits.scene_file())?
            .canonicalize(self.validation.registry())
            .map_err(map_scene_publication_error)?;
        self.prepare_component_asset_references(&canonical.document().entities)?;
        let published = if self.catalogue.is_some() {
            canonical.publish_with_asset_database(self.validation.registry(), &self.asset_database)
        } else {
            canonical.publish(self.validation.registry())
        }
        .map_err(map_scene_publication_error)?;
        self.source_upgrade_required |= published.source_upgrade_required();
        let document = published.into_document();
        let retained = prefab_retained_bytes(&document)?
            .checked_add(ARC_CONTROL_BLOCK_BYTES)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
        self.commit_work_as_retained(work_before, retained)?;
        Ok(document)
    }

    fn enqueue_prefab_references(
        &mut self,
        entity: &SceneEntityRecord,
        depth: usize,
        queue: &mut VecDeque<(AssetPath, usize)>,
        discovered: &mut BTreeSet<AssetPath>,
    ) -> Result<(), ProjectContentError> {
        if let Some(prefab) = &entity.prefab {
            self.enqueue_prefab_ref(&prefab.source, depth, queue, discovered)?;
            self.enqueue_patch_prefabs(&prefab.overrides, depth, queue, discovered)?;
        }
        Ok(())
    }

    fn enqueue_patch_prefabs(
        &mut self,
        patch: &ScenePatchDocument,
        depth: usize,
        queue: &mut VecDeque<(AssetPath, usize)>,
        discovered: &mut BTreeSet<AssetPath>,
    ) -> Result<(), ProjectContentError> {
        for operation in &patch.operations {
            match operation {
                ScenePatchOperation::AddEntity { entity, .. } => {
                    self.prepare_component_asset_references(std::slice::from_ref(entity))?;
                    self.enqueue_prefab_references(entity, depth, queue, discovered)?;
                }
                ScenePatchOperation::RestoreEntity {
                    entity,
                    descendants,
                    ..
                } => {
                    self.prepare_component_asset_references(std::slice::from_ref(entity))?;
                    self.prepare_component_asset_references(descendants)?;
                    self.enqueue_prefab_references(entity, depth, queue, discovered)?;
                    for descendant in descendants {
                        self.enqueue_prefab_references(descendant, depth, queue, discovered)?;
                    }
                }
                ScenePatchOperation::SetAssetRefField {
                    asset_ref: AssetRef::StableId(_),
                    ..
                } => {
                    self.ensure_reference_catalogue()?;
                }
                ScenePatchOperation::SetField {
                    component,
                    field,
                    value,
                    ..
                } => {
                    let field_schema =
                        self.validation
                            .registry()
                            .schema(component)
                            .and_then(|schema| {
                                schema
                                    .fields()
                                    .iter()
                                    .find(|candidate| candidate.id() == field)
                            });
                    let stable_asset = if let Some(field_schema) = field_schema {
                        collect_declared_field_asset_references(component, field_schema, value)
                            .map_err(map_asset_reference_error)?
                            .iter()
                            .any(|reference| matches!(reference.asset_ref(), AssetRef::StableId(_)))
                    } else {
                        false
                    };
                    if stable_asset {
                        self.ensure_reference_catalogue()?;
                    }
                }
                ScenePatchOperation::AddComponent {
                    component, value, ..
                }
                | ScenePatchOperation::ReplaceComponent {
                    component, value, ..
                } => {
                    let references = collect_declared_asset_references(
                        self.validation.registry(),
                        component,
                        value.version,
                        &value.value,
                    )
                    .map_err(map_asset_reference_error)?;
                    if references
                        .iter()
                        .any(|reference| matches!(reference.asset_ref(), AssetRef::StableId(_)))
                    {
                        self.ensure_reference_catalogue()?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn enqueue_prefab_ref(
        &mut self,
        source: &AssetRef,
        depth: usize,
        queue: &mut VecDeque<(AssetPath, usize)>,
        discovered: &mut BTreeSet<AssetPath>,
    ) -> Result<(), ProjectContentError> {
        self.add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
        self.observe_depth(depth)?;
        let path = self.resolve_reference_path(source, &AssetSourceKind::Prefab)?;
        self.observe_path(path.as_str())?;
        if !discovered.contains(&path) {
            self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
            discovered.insert(path.clone());
            queue.push_back((path.clone(), depth));
        }
        Ok(())
    }

    fn collect_source_candidates(
        &mut self,
        document: &SceneDocument,
        assets: &DirectoryCapability,
    ) -> Result<SourceCandidates, ProjectContentError> {
        let mut sources = SourceCandidates::default();
        for entity in &document.entities {
            for (component, record) in &entity.components {
                let references = collect_declared_asset_references(
                    self.validation.registry(),
                    component,
                    record.version,
                    &record.value,
                )
                .map_err(map_asset_reference_error)?;
                for reference in references {
                    self.add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
                    let source = self.discover_source_reference(assets, reference.asset_ref())?;
                    let path = source.path();
                    self.observe_path(path.as_str())?;
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
                    if !paths.contains(path) {
                        self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
                        paths.insert(path.clone());
                    }
                }
            }
        }
        Ok(sources)
    }

    fn prepare_component_asset_references(
        &mut self,
        entities: &[SceneEntityRecord],
    ) -> Result<(), ProjectContentError> {
        for entity in entities {
            for (component, record) in &entity.components {
                let references = collect_declared_asset_references(
                    self.validation.registry(),
                    component,
                    record.version,
                    &record.value,
                )
                .map_err(map_asset_reference_error)?;
                if references
                    .iter()
                    .any(|reference| matches!(reference.asset_ref(), AssetRef::StableId(_)))
                {
                    self.ensure_reference_catalogue()?;
                }
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
        let result_reservation = self.reserve_work(result_bytes)?;
        let mut tilesets = Vec::new();
        tilesets
            .try_reserve_exact(paths.len())
            .map_err(|_| allocation_failed("project.content.tileset-list-allocation-failed"))?;
        for path in paths {
            self.subtract_budget(ProjectContentBudgetKind::QueuedJobs, 1);
            self.add_budget(ProjectContentBudgetKind::InFlightJobs, 1)?;
            let result = self.load_tileset(assets, asset_root, &path);
            self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            tilesets.push(result?);
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
                .find(|image| image.path() == &tileset.image_path)
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
            let result = validate_persisted_tilemap_membership(&record.value, |reference| {
                tilesets
                    .iter()
                    .find(|tileset| match reference {
                        AssetRef::Path(path) => &tileset.path == path,
                        AssetRef::StableId(id) => &tileset.stable_id == id,
                    })
                    .map(|tileset| tileset.source.atlas)
            });
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
    ) -> Result<ProjectTileSetContent, ProjectContentError> {
        let meta_path = path.meta_path();
        let (meta_bytes, meta_digest) = self.read_file(
            assets,
            &meta_path,
            nara_asset::AssetMetaFileLimits::default()
                .encoded_bytes()
                .get(),
        )?;
        self.verify_catalogued_metadata(path, asset_root, meta_digest)?;
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
        let record = AssetRecord::from(identity);
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
        let image_path = self.resolve_reference_path(&source.image, &AssetSourceKind::Image)?;
        self.add_budget(ProjectContentBudgetKind::DependencyEdges, 1)?;
        self.verify_catalogued_metadata(path, asset_root, meta_digest)?;
        self.record_member(
            ProjectContentMemberKind::AssetSource,
            asset_root,
            path.as_str(),
            source_digest,
        )?;
        Ok(ProjectTileSetContent {
            stable_id,
            path: path.clone(),
            image_path,
            metadata_digest: meta_digest,
            source_hash: source_digest,
            source,
        })
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
        self.verify_catalogued_metadata(path, asset_root, meta_digest)?;
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
        let record = AssetRecord::from(identity);
        if let Some(indexed) = self.asset_database.record_for_path(path) {
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
        let work_before = self.reserve_image_memory(path, memory)?;
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
        path: &AssetPath,
        memory: ImageImportMemoryPlan,
    ) -> Result<usize, ProjectContentError> {
        let work_before = self.budget_value(ProjectContentBudgetKind::WorkBytes);
        let work_bytes = work_before
            .checked_add(memory.decoder_work_bytes())
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let artifact_bytes = self
            .budget_value(ProjectContentBudgetKind::ArtifactBytes)
            .checked_add(memory.rgba_bytes())
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::ArtifactBytes))?;
        let retained_bytes = self
            .budget_value(ProjectContentBudgetKind::RetainedBytes)
            .checked_add(image_retained_plan_bytes(path, memory.rgba_bytes())?)
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
        Ok(WorkReservation {
            previous,
            reserved: amount,
        })
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
        debug_assert!(remaining_work >= reservation.previous);
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
    match error {
        SceneFormatError::LegacyPlacementShape { entity_index } => {
            ProjectContentError::legacy_placement_shape(
                "project.content.scene-legacy-placement-shape",
                "Rewrite the legacy scene with ordered roots and parent-owned children; sibling order cannot be inferred",
                "scene",
                path,
                entity_index,
            )
        }
        _ => ProjectContentError::with_identifier(
            ProjectContentErrorKind::SceneFormat,
            "project.content.scene-format-invalid",
            "Project scene format is invalid",
            "scene",
            path,
        ),
    }
}

fn map_prefab_format_error(path: &str, error: SceneFormatError) -> ProjectContentError {
    match error {
        SceneFormatError::LegacyPlacementShape { entity_index } => {
            ProjectContentError::legacy_placement_shape(
                "project.content.prefab-legacy-placement-shape",
                "Rewrite the legacy prefab with ordered roots and parent-owned children; sibling order cannot be inferred",
                "prefab",
                path,
                entity_index,
            )
        }
        _ => ProjectContentError::with_identifier(
            ProjectContentErrorKind::SceneFormat,
            "project.content.prefab-format-invalid",
            "Project prefab format is invalid",
            "prefab",
            path,
        ),
    }
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
                PublicDiagnosticIdentifier::new(source.source_entity_id().as_str()).unwrap_or_else(
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

struct LoadedPrefabResolver<'a> {
    prefabs: &'a BTreeMap<AssetPath, Arc<PrefabDocument>>,
    database: &'a ProjectAssetDatabase,
}

impl<'a> LoadedPrefabResolver<'a> {
    const fn new(
        prefabs: &'a BTreeMap<AssetPath, Arc<PrefabDocument>>,
        database: &'a ProjectAssetDatabase,
    ) -> Self {
        Self { prefabs, database }
    }
}

impl PrefabSourceResolver for LoadedPrefabResolver<'_> {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        let path = match source {
            AssetRef::Path(path) => path,
            AssetRef::StableId(_) => self.database.resolve_ref(source).ok()?.path(),
        };
        self.prefabs.get(path).map(Arc::as_ref)
    }
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
const PATCH_OPERATION_WORK_BYTES: usize = 128;
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
                limits.patch_operations().get(),
                PATCH_OPERATION_WORK_BYTES,
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
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.materialized_entities().get(),
                ENTITY_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.materialized_components().get(),
                COMPONENT_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.materialized_value_nodes().get(),
                VALUE_NODE_ALLOCATION_BYTES,
            )?,
            limits.materialized_value_bytes().get(),
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.resolved_instances().get(),
                PREFAB_INSTANCE_WORK_BYTES,
            )?,
            checked_plan_product(
                ProjectContentBudgetKind::WorkBytes,
                limits.applied_patch_operations().get(),
                PATCH_OPERATION_WORK_BYTES,
            )?,
            limits.generated_identifier_bytes().get(),
        ],
    )
}

pub(crate) fn startup_scene_retained_bytes(
    document: &SceneDocument,
    provenance: &SceneEntityProvenanceMap,
) -> Result<usize, ProjectContentError> {
    crate::scene_retention::startup_scene_retained_bytes(document, provenance)
        .map_err(|_| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn editor_startup_scene_retained_bytes(
    document: &SceneDocument,
    provenance: &SceneEntityProvenanceMap,
) -> Result<usize, ProjectContentError> {
    crate::scene_retention::direct_startup_scene_retained_bytes(document, provenance)
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

#[cfg(test)]
fn patch_retained_bytes(
    patch: &nara_scene::ScenePatchDocument,
) -> Result<usize, ProjectContentError> {
    crate::scene_retention::patch_retained_bytes(patch)
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
    path: &AssetPath,
    rgba_bytes: usize,
) -> Result<usize, ProjectContentError> {
    rgba_bytes
        .checked_add(string_retained_bytes(path.as_str())?)
        .and_then(|bytes| bytes.checked_add(ARC_CONTROL_BLOCK_BYTES))
        .and_then(|bytes| bytes.checked_add(IMAGE_METADATA_OVERHEAD))
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
}

fn snapshot_retained_overhead(
    prefabs: &BTreeMap<AssetPath, Arc<PrefabDocument>>,
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
    for path in prefabs.keys() {
        checked_add_to(
            &mut total,
            string_retained_bytes(path.as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    for tileset in tilesets {
        checked_add_to(
            &mut total,
            string_retained_bytes(tileset.path.as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
        checked_add_to(
            &mut total,
            string_retained_bytes(tileset.image_path.as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
        checked_add_to(
            &mut total,
            string_retained_bytes(&tileset.source.image.to_string())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
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
    use std::mem::size_of;

    use nara_identity::SceneEntityId;
    use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId};
    use nara_scene::{
        ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    };

    use super::{
        ProjectContentBudgetKind, checked_plan_sum, patch_retained_bytes, string_retained_bytes,
    };

    #[test]
    fn patch_retained_plan_counts_every_owned_identifier() {
        let entity = SceneEntityId::new("entity-with-owned-id").unwrap();
        let parent = SceneEntityId::new("parent-with-owned-id").unwrap();
        let component = ComponentTypeId::new("nara.test.ComponentWithOwnedId");
        let field = ComponentFieldId::new("field-with-owned-id");
        let patch = ScenePatchDocument::new([
            ScenePatchOperation::RemoveComponent {
                entity: entity.clone(),
                component: component.clone(),
            },
            ScenePatchOperation::RemoveField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: ComponentSchemaVersion::ONE,
                field: field.clone(),
            },
            ScenePatchOperation::MoveEntity {
                entity: entity.clone(),
                from: ScenePlacement::last(ScenePlacementOwner::Root),
                to: ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: parent.clone(),
                }),
            },
            ScenePatchOperation::MoveEntity {
                entity: entity.clone(),
                from: ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: parent.clone(),
                }),
                to: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        ]);
        let expected = checked_plan_sum(
            ProjectContentBudgetKind::RetainedBytes,
            [
                4 * size_of::<ScenePatchOperation>(),
                string_retained_bytes(entity.as_str()).unwrap(),
                string_retained_bytes(component.as_str()).unwrap(),
                string_retained_bytes(entity.as_str()).unwrap(),
                string_retained_bytes(component.as_str()).unwrap(),
                string_retained_bytes(field.as_str()).unwrap(),
                string_retained_bytes(entity.as_str()).unwrap(),
                string_retained_bytes(parent.as_str()).unwrap(),
                string_retained_bytes(entity.as_str()).unwrap(),
                string_retained_bytes(parent.as_str()).unwrap(),
            ],
        )
        .unwrap();

        assert_eq!(patch_retained_bytes(&patch).unwrap(), expected);
    }
}
