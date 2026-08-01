//! Authorized, World-independent startup content loading.

mod budget;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt::{self, Display, Formatter},
    mem::{size_of, size_of_val},
    sync::Arc,
};

use nara_asset::{
    AssetMetaCandidate, AssetPath, AssetRecord, AssetRef, AssetSourceKind, ImportDependencyDigest,
    ImportProfile, ImportSettingsHash, ProjectAssetDatabase,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_fs::{ContentDigest, DirectoryCapability, FsError, FsOperation, RelativePath};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageImportError, ImageImportMemoryPlan, ImageImporter,
    ImageImporterCreateError,
};
use nara_reflect::__private::{
    ComponentRegistrySnapshotWitness, component_registry_snapshot_witness,
    component_registry_snapshot_witness_matches,
};
use nara_reflect::{
    ComponentRegistrySnapshot, DeclaredAssetReferenceError, SchemaCompositionFingerprint,
    collect_declared_asset_references,
};
use nara_scene::{
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, PrefabSourceResolver, SceneDocument,
    SceneDocumentCandidate, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};

use crate::project_diagnostic_ids::{fs_operation_id, io_error_kind_id};
use crate::project_host::{
    ProjectSettingsCandidate, ProjectSettingsLineage, RuntimePlan, SchemaValidationInput,
};

use budget::{BudgetTicket, ProjectContentLease};
pub use budget::{
    ProjectContentBudgetError, ProjectContentBudgetHost, ProjectContentBudgetKind,
    ProjectContentBudgetSnapshot, ProjectContentLimits,
};

// nara_fs relative traversal retains at most one intermediate handle while opening the next, and
// bounded reads clone the opened file for the reader. Reserve both slots before either operation.
const RELATIVE_OPEN_HANDLE_PEAK: usize = 2;
const RETAINED_DIRECTORY_HANDLES: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Clone)]
pub struct ProjectContentSnapshot {
    inner: Arc<ProjectContentSnapshotInner>,
}

struct ProjectContentSnapshotInner {
    lineage: ProjectSettingsLineage,
    schema_fingerprint: SchemaCompositionFingerprint,
    schema_authority: ComponentRegistrySnapshotWitness,
    revision: ProjectContentRevision,
    content_digest: ContentDigest,
    source_upgrade_required: bool,
    startup_scene: Arc<SceneDocument>,
    expanded_startup_scene: Arc<SceneDocument>,
    prefabs: Box<[ProjectPrefabContent]>,
    images: Box<[ProjectImageContent]>,
    _lease: ProjectContentLease,
}

impl ProjectContentSnapshot {
    #[must_use]
    pub fn lineage(&self) -> ProjectSettingsLineage {
        self.inner.lineage
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.inner.schema_fingerprint
    }

    #[must_use]
    pub(crate) fn shares_schema_snapshot(&self, snapshot: &ComponentRegistrySnapshot) -> bool {
        component_registry_snapshot_witness_matches(&self.inner.schema_authority, snapshot)
    }

    #[must_use]
    pub fn revision(&self) -> ProjectContentRevision {
        self.inner.revision
    }

    #[must_use]
    pub fn content_digest(&self) -> ContentDigest {
        self.inner.content_digest
    }

    #[must_use]
    pub fn source_upgrade_required(&self) -> bool {
        self.inner.source_upgrade_required
    }

    #[must_use]
    pub fn startup_scene(&self) -> &SceneDocument {
        &self.inner.startup_scene
    }

    #[must_use]
    pub fn expanded_startup_scene(&self) -> &SceneDocument {
        &self.inner.expanded_startup_scene
    }

    #[must_use]
    pub fn prefabs(&self) -> &[ProjectPrefabContent] {
        &self.inner.prefabs
    }

    #[must_use]
    pub fn images(&self) -> &[ProjectImageContent] {
        &self.inner.images
    }

    pub(crate) fn share_image_for_runtime(&self, index: usize) -> Option<ImageAsset> {
        self.inner
            .images
            .get(index)
            .and_then(|content| content.image.share_retained())
    }

    pub(crate) fn prepare_editor_startup_scene(
        &self,
        document: &SceneDocument,
        registry: &nara_reflect::ComponentRegistry,
    ) -> Result<SceneDocument, ProjectContentError> {
        let resolver = SnapshotPrefabResolver {
            prefabs: self.prefabs(),
        };
        let expansion = document.expand_prefabs_with_options(
            registry,
            &resolver,
            nara_scene::PrefabExpansionOptions::default(),
        );
        let expanded = expansion.document.ok_or_else(|| {
            map_scene_diagnostics(
                ProjectContentErrorKind::PrefabExpansion,
                expansion.diagnostics,
            )
        })?;
        let diagnostics = expanded.validate(registry);
        if diagnostics.has_errors() {
            return Err(map_scene_diagnostics(
                ProjectContentErrorKind::ScenePublication,
                diagnostics,
            ));
        }
        Ok(expanded)
    }
}

struct SnapshotPrefabResolver<'a> {
    prefabs: &'a [ProjectPrefabContent],
}

impl PrefabSourceResolver for SnapshotPrefabResolver<'_> {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        let AssetRef::Path(path) = source else {
            return None;
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
            .field("lineage", &self.inner.lineage)
            .field("schema_fingerprint", &self.inner.schema_fingerprint)
            .field("revision", &self.inner.revision)
            .field("content_digest", &self.inner.content_digest)
            .field("prefab_count", &self.inner.prefabs.len())
            .field("image_count", &self.inner.images.len())
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
    InvalidLogicalPath,
    HostIo,
    HostAuthorityUnsupported,
    HostAuthorityUnproven,
    HostAuthorityRejected,
    BudgetExceeded,
    SceneFormat,
    ScenePublication,
    PrefabExpansion,
    AssetReference,
    UnsupportedStableAssetReference,
    AssetMeta,
    AssetMetaMismatch,
    UnsupportedAssetKind,
    ImageImport,
    AllocationFailed,
    UnsupportedHierarchySemantics,
    SchemaMismatch,
}

#[derive(Clone, PartialEq)]
pub struct ProjectContentError {
    kind: ProjectContentErrorKind,
    diagnostics: Box<DiagnosticReport>,
}

impl ProjectContentError {
    #[must_use]
    pub const fn kind(&self) -> ProjectContentErrorKind {
        self.kind
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        self.diagnostics.as_ref()
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
            ProjectContentErrorKind::MissingStartupScene => "project has no startup scene",
            ProjectContentErrorKind::InvalidLogicalPath => {
                "project content contains an invalid logical path"
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
            ProjectContentErrorKind::UnsupportedStableAssetReference => {
                "stable asset references are unsupported for startup content"
            }
            ProjectContentErrorKind::AssetMeta => "project asset metadata is invalid",
            ProjectContentErrorKind::AssetMetaMismatch => {
                "project asset metadata does not match its source"
            }
            ProjectContentErrorKind::UnsupportedAssetKind => {
                "project startup content references an unsupported asset kind"
            }
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

    pub fn load(
        &self,
        candidate: &ProjectSettingsCandidate,
        plan: &RuntimePlan,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.validate_inputs(candidate, plan)?;
        let validation = plan.schema_validation();
        let mut context = LoadContext::new(self, validation);
        context.load(candidate)
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

struct LoadContext<'a> {
    loader: &'a ProjectContentLoader,
    validation: &'a SchemaValidationInput,
    budget: Option<BudgetTicket>,
    digest: ContentDigestBuilder,
    digest_work_bytes: usize,
    source_upgrade_required: bool,
    asset_database: ProjectAssetDatabase,
}

#[derive(Debug, Clone, Copy)]
struct WorkReservation {
    previous: usize,
    reserved: usize,
}

impl<'a> LoadContext<'a> {
    fn new(loader: &'a ProjectContentLoader, validation: &'a SchemaValidationInput) -> Self {
        Self {
            loader,
            validation,
            budget: Some(loader.budget_host.reserve()),
            digest: ContentDigestBuilder::default(),
            digest_work_bytes: 0,
            source_upgrade_required: false,
            asset_database: ProjectAssetDatabase::default(),
        }
    }

    fn load(
        &mut self,
        candidate: &ProjectSettingsCandidate,
    ) -> Result<ProjectContentSnapshot, ProjectContentError> {
        self.add_budget(ProjectContentBudgetKind::OpenHandles, 1)?;
        let settings = candidate.settings();
        let scenes = self.open_directory(&settings.paths.scenes)?;
        let prefabs = self.open_directory(&settings.paths.prefabs)?;
        let assets = self.open_directory(&settings.paths.assets)?;
        let startup = settings.startup.default_scene.as_ref().ok_or_else(|| {
            ProjectContentError::single(
                ProjectContentErrorKind::MissingStartupScene,
                "project.content.startup-scene-missing",
                "Project manifest does not select a startup scene",
            )
        })?;

        let (scene_bytes, scene_digest) = self.read_file(
            &scenes,
            startup.as_str(),
            self.loader.limits.scene_file().encoded_bytes().get(),
        )?;
        self.record_digest("scene", startup.as_str(), scene_digest)?;
        let startup_scene = self.decode_scene(startup.as_str(), &scene_bytes)?;

        let loaded_prefabs = self.load_prefab_closure(&prefabs, &startup_scene)?;
        let resolver = LoadedPrefabResolver::new(&loaded_prefabs);
        let expansion_work = prefab_expansion_work_plan(self.loader.limits.prefab_expansion())?;
        let work_before = self.reserve_work(expansion_work)?;
        let expansion = startup_scene.expand_prefabs_with_options(
            self.validation.registry(),
            &resolver,
            nara_scene::PrefabExpansionOptions::default()
                .with_limits(self.loader.limits.prefab_expansion()),
        );
        let expanded = expansion.document.ok_or_else(|| {
            map_scene_diagnostics(
                ProjectContentErrorKind::PrefabExpansion,
                expansion.diagnostics,
            )
        })?;
        let expanded_retained = scene_retained_bytes(&expanded)?;
        self.commit_work_as_retained(work_before, expanded_retained)?;

        let asset_refs = self.collect_image_candidates(&expanded)?;
        let images = self.load_images(&assets, asset_refs)?;
        drop((scenes, prefabs, assets));
        let open_handles = self.budget_value(ProjectContentBudgetKind::OpenHandles);
        self.subtract_budget(ProjectContentBudgetKind::OpenHandles, open_handles);

        let content_digest = self.finish_content_digest()?;
        let revision = content_revision(
            candidate.lineage(),
            self.validation.composition_fingerprint(),
            content_digest,
        )?;

        let snapshot_overhead = snapshot_retained_overhead(&loaded_prefabs, images.as_slice())?;
        let retained_bytes = self
            .budget_value(ProjectContentBudgetKind::RetainedBytes)
            .checked_add(snapshot_overhead)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))?;
        self.set_budget(ProjectContentBudgetKind::RetainedBytes, retained_bytes)?;

        let startup_scene = Arc::new(startup_scene);
        let expanded_startup_scene = Arc::new(expanded);
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

        let budget = self
            .budget
            .take()
            .expect("load context owns one budget ticket until publication");
        let lease = budget.into_lease().map_err(ProjectContentError::budget)?;
        if !images.is_empty() {
            let image_retention = Arc::new(lease.clone());
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

        Ok(ProjectContentSnapshot {
            inner: Arc::new(ProjectContentSnapshotInner {
                lineage: candidate.lineage(),
                schema_fingerprint: self.validation.composition_fingerprint(),
                schema_authority: component_registry_snapshot_witness(self.validation.snapshot()),
                revision,
                content_digest,
                source_upgrade_required: self.source_upgrade_required,
                startup_scene,
                expanded_startup_scene,
                prefabs,
                images,
                _lease: lease,
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
            let loaded_result = self.load_prefab(prefabs, &path);
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
        path: &AssetPath,
    ) -> Result<PrefabDocument, ProjectContentError> {
        let (bytes, digest) = self.read_file(
            prefabs,
            path.as_str(),
            self.loader.limits.scene_file().encoded_bytes().get(),
        )?;
        self.record_digest("prefab", path.as_str(), digest)?;
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
        self.reject_stable_component_asset_references(&canonical.document().entities)?;
        let published = canonical
            .publish(self.validation.registry())
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
        self.reject_stable_component_asset_references(&canonical.document().entities)?;
        let published = canonical
            .publish(self.validation.registry())
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
            if let ScenePatchOperation::AddEntity { entity } = operation {
                self.enqueue_prefab_references(entity, depth, queue, discovered)?;
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
        let AssetRef::Path(path) = source else {
            return Err(ProjectContentError::single(
                ProjectContentErrorKind::UnsupportedStableAssetReference,
                "project.content.stable-asset-reference-unsupported",
                "Stable asset references require a bounded catalogue",
            ));
        };
        self.observe_path(path.as_str())?;
        if !discovered.contains(path) {
            self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
            discovered.insert(path.clone());
            queue.push_back((path.clone(), depth));
        }
        Ok(())
    }

    fn collect_image_candidates(
        &mut self,
        document: &SceneDocument,
    ) -> Result<BTreeSet<AssetPath>, ProjectContentError> {
        let mut paths = BTreeSet::new();
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
                    let AssetRef::Path(path) = reference.asset_ref() else {
                        return Err(ProjectContentError::single(
                            ProjectContentErrorKind::UnsupportedStableAssetReference,
                            "project.content.stable-asset-reference-unsupported",
                            "Stable asset references require a bounded catalogue",
                        ));
                    };
                    self.observe_path(path.as_str())?;
                    if !paths.contains(path) {
                        self.add_budget(ProjectContentBudgetKind::QueuedJobs, 1)?;
                        paths.insert(path.clone());
                    }
                }
            }
        }
        Ok(paths)
    }

    fn reject_stable_component_asset_references(
        &self,
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
                    return Err(ProjectContentError::single(
                        ProjectContentErrorKind::UnsupportedStableAssetReference,
                        "project.content.stable-asset-reference-unsupported",
                        "Stable asset references require a bounded catalogue",
                    ));
                }
            }
        }
        Ok(())
    }

    fn load_images(
        &mut self,
        assets: &DirectoryCapability,
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
            let result = self.load_image(assets, &path);
            self.subtract_budget(ProjectContentBudgetKind::InFlightJobs, 1);
            images.push(result?);
        }
        self.commit_work_as_retained(result_reservation, result_bytes)?;
        Ok(images)
    }

    fn load_image(
        &mut self,
        assets: &DirectoryCapability,
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
        self.record_digest("asset-meta", &meta_path, meta_digest)?;
        let meta = AssetMetaCandidate::decode_json_bytes(&meta_bytes)
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
        if &meta.path != path {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.asset-meta-path-mismatch",
                "Project asset metadata path does not match its source",
                "asset",
                path.as_str(),
            ));
        }
        if meta.source_kind != AssetSourceKind::Image {
            return Err(ProjectContentError::with_identifier(
                ProjectContentErrorKind::UnsupportedAssetKind,
                "project.content.asset-kind-unsupported",
                "Project startup content references an unsupported asset kind",
                "asset",
                path.as_str(),
            ));
        }
        let record = AssetRecord::from(meta);
        self.asset_database.insert(record.clone()).map_err(|_| {
            ProjectContentError::with_identifier(
                ProjectContentErrorKind::AssetMetaMismatch,
                "project.content.asset-identity-conflict",
                "Project asset identity conflicts within the startup closure",
                "asset",
                path.as_str(),
            )
        })?;
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
                ImportDependencyDigest::empty(),
                ImportSettingsHash::default(),
                ImportProfile::default(),
            ))
            .map_err(map_image_import_error)?;
        let memory = import.memory_plan();
        let work_before = self.reserve_image_memory(path, memory)?;
        let image = import.import().map_err(map_image_import_error)?;
        self.set_budget(ProjectContentBudgetKind::WorkBytes, work_before)?;
        self.record_digest("asset", path.as_str(), source_digest)?;
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

    fn record_digest(
        &mut self,
        role: &str,
        path: &str,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let work = digest_entry_work_bytes(role, path)?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, work)?;
        self.digest.insert(role, path, digest)?;
        self.digest_work_bytes = self
            .digest_work_bytes
            .checked_add(work)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        Ok(())
    }

    fn finish_content_digest(&mut self) -> Result<ContentDigest, ProjectContentError> {
        let frame_work = self.digest.framed_bytes()?;
        self.add_budget(ProjectContentBudgetKind::WorkBytes, frame_work)?;
        let release = self
            .digest_work_bytes
            .checked_add(frame_work)
            .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))?;
        let digest = std::mem::take(&mut self.digest).finish(frame_work)?;
        self.subtract_budget(ProjectContentBudgetKind::WorkBytes, release);
        self.digest_work_bytes = 0;
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
    .map_err(|_| {
        ProjectContentError::with_identifier(
            ProjectContentErrorKind::SceneFormat,
            "project.content.scene-format-invalid",
            "Project scene format is invalid",
            "scene",
            path,
        )
    })
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
    .map_err(|_| {
        ProjectContentError::with_identifier(
            ProjectContentErrorKind::SceneFormat,
            "project.content.prefab-format-invalid",
            "Project prefab format is invalid",
            "prefab",
            path,
        )
    })
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

struct LoadedPrefabResolver<'a> {
    prefabs: &'a BTreeMap<AssetPath, Arc<PrefabDocument>>,
}

impl<'a> LoadedPrefabResolver<'a> {
    const fn new(prefabs: &'a BTreeMap<AssetPath, Arc<PrefabDocument>>) -> Self {
        Self { prefabs }
    }
}

impl PrefabSourceResolver for LoadedPrefabResolver<'_> {
    fn resolve_prefab(&self, source: &AssetRef) -> Option<&PrefabDocument> {
        let AssetRef::Path(path) = source else {
            return None;
        };
        self.prefabs.get(path).map(Arc::as_ref)
    }
}

#[derive(Default)]
struct ContentDigestBuilder {
    entries: BTreeMap<(String, String), (usize, [u8; 32])>,
}

impl ContentDigestBuilder {
    fn insert(
        &mut self,
        role: &str,
        path: &str,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let length = usize::try_from(digest.length())
            .map_err(|_| overflow_budget(ProjectContentBudgetKind::EncodedBytes))?;
        self.insert_hash(role, path, length, *digest.as_bytes())
    }

    fn insert_hash(
        &mut self,
        role: &str,
        path: &str,
        length: usize,
        hash: [u8; 32],
    ) -> Result<(), ProjectContentError> {
        let key = (role.to_owned(), path.to_owned());
        match self.entries.insert(key, (length, hash)) {
            None => Ok(()),
            Some(previous) if previous == (length, hash) => Ok(()),
            Some(_) => Err(ProjectContentError::single(
                ProjectContentErrorKind::HostIo,
                "project.content.source-changed",
                "Project content source changed during snapshot construction",
            )),
        }
    }

    fn framed_bytes(&self) -> Result<usize, ProjectContentError> {
        let mut total = b"nara.project-content-digest.v1\0".len();
        for (role, path) in self.entries.keys() {
            total = checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [total, 8, role.len(), 8, path.len(), 8, 32],
            )?;
        }
        Ok(total)
    }

    fn finish(self, framed_bytes: usize) -> Result<ContentDigest, ProjectContentError> {
        let mut framed = Vec::new();
        framed.try_reserve_exact(framed_bytes).map_err(|_| {
            ProjectContentError::single(
                ProjectContentErrorKind::AllocationFailed,
                "project.content.digest-allocation-failed",
                "Project content digest allocation failed",
            )
        })?;
        let mut encoded_length = 0_u64;
        framed.extend_from_slice(b"nara.project-content-digest.v1\0");
        for ((role, path), (length, hash)) in self.entries {
            append_frame(&mut framed, role.as_bytes())?;
            append_frame(&mut framed, path.as_bytes())?;
            let length = u64::try_from(length)
                .map_err(|_| overflow_budget(ProjectContentBudgetKind::EncodedBytes))?;
            framed.extend_from_slice(&length.to_le_bytes());
            framed.extend_from_slice(&hash);
            encoded_length = encoded_length
                .checked_add(length)
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::EncodedBytes))?;
        }
        debug_assert_eq!(framed.len(), framed_bytes);
        let hash = ContentDigest::of_bytes(&framed);
        Ok(ContentDigest::from_parts(encoded_length, *hash.as_bytes()))
    }
}

fn content_revision(
    lineage: ProjectSettingsLineage,
    schema: SchemaCompositionFingerprint,
    content: ContentDigest,
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

fn digest_entry_work_bytes(role: &str, path: &str) -> Result<usize, ProjectContentError> {
    checked_plan_sum(
        ProjectContentBudgetKind::WorkBytes,
        [
            size_of::<((String, String), (usize, [u8; 32]))>(),
            BTREE_ENTRY_OVERHEAD,
            string_work_bytes(role)?,
            string_work_bytes(path)?,
        ],
    )
}

fn string_work_bytes(value: &str) -> Result<usize, ProjectContentError> {
    value
        .len()
        .checked_add(STRING_ALLOCATION_OVERHEAD)
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::WorkBytes))
}

const STRING_ALLOCATION_OVERHEAD: usize = 2 * size_of::<usize>();
const BTREE_ENTRY_OVERHEAD: usize = 6 * size_of::<usize>();
const ARC_CONTROL_BLOCK_BYTES: usize = 4 * size_of::<usize>();
const SNAPSHOT_LEASE_OVERHEAD: usize = 128;
const IMAGE_RETENTION_OWNER_OVERHEAD: usize =
    ARC_CONTROL_BLOCK_BYTES + size_of::<ProjectContentLease>();
const IMAGE_METADATA_OVERHEAD: usize = 256;
const VALUE_NODE_ALLOCATION_BYTES: usize = 32;
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

fn scene_retained_bytes(document: &SceneDocument) -> Result<usize, ProjectContentError> {
    document_retained_bytes(size_of::<SceneDocument>(), &document.entities)
}

fn prefab_retained_bytes(document: &PrefabDocument) -> Result<usize, ProjectContentError> {
    document_retained_bytes(size_of::<PrefabDocument>(), &document.entities)
}

fn document_retained_bytes(
    document_bytes: usize,
    entities: &[SceneEntityRecord],
) -> Result<usize, ProjectContentError> {
    let mut total = checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [
            document_bytes,
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                entities.len(),
                size_of::<SceneEntityRecord>(),
            )?,
        ],
    )?;
    for entity in entities {
        checked_add_to(
            &mut total,
            entity_dynamic_bytes(entity)?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    Ok(total)
}

fn entity_dynamic_bytes(entity: &SceneEntityRecord) -> Result<usize, ProjectContentError> {
    let mut total = string_retained_bytes(entity.id.as_str())?;
    if let Some(parent) = &entity.parent {
        checked_add_to(
            &mut total,
            string_retained_bytes(parent.as_str())?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    for (id, record) in &entity.components {
        checked_add_to(
            &mut total,
            checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    size_of_val(id),
                    size_of_val(record),
                    BTREE_ENTRY_OVERHEAD,
                    string_retained_bytes(id.as_str())?,
                    component_value_retained_bytes(&record.value)?,
                ],
            )?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    if let Some(instance) = &entity.prefab {
        checked_add_to(
            &mut total,
            prefab_instance_retained_bytes(instance)?,
            ProjectContentBudgetKind::RetainedBytes,
        )?;
    }
    Ok(total)
}

fn prefab_instance_retained_bytes(instance: &PrefabInstance) -> Result<usize, ProjectContentError> {
    let source = match &instance.source {
        AssetRef::Path(path) => string_retained_bytes(path.as_str())?,
        AssetRef::StableId(_) => 16,
    };
    checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [source, patch_retained_bytes(&instance.overrides)?],
    )
}

fn patch_retained_bytes(patch: &ScenePatchDocument) -> Result<usize, ProjectContentError> {
    let mut total = checked_plan_product(
        ProjectContentBudgetKind::RetainedBytes,
        patch.operations.len(),
        size_of::<ScenePatchOperation>(),
    )?;
    for operation in &patch.operations {
        let dynamic = match operation {
            ScenePatchOperation::AddEntity { entity } => entity_dynamic_bytes(entity)?,
            ScenePatchOperation::AddComponent {
                entity,
                component,
                value,
            }
            | ScenePatchOperation::ReplaceComponent {
                entity,
                component,
                value,
            } => checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    string_retained_bytes(entity.as_str())?,
                    string_retained_bytes(component.as_str())?,
                    component_value_retained_bytes(&value.value)?,
                ],
            )?,
            ScenePatchOperation::SetField {
                entity,
                component,
                field,
                value,
                ..
            } => checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    string_retained_bytes(entity.as_str())?,
                    string_retained_bytes(component.as_str())?,
                    string_retained_bytes(field.as_str())?,
                    component_value_retained_bytes(value)?,
                ],
            )?,
            ScenePatchOperation::SetAssetRefField {
                entity,
                component,
                field,
                asset_ref,
                ..
            } => checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    string_retained_bytes(entity.as_str())?,
                    string_retained_bytes(component.as_str())?,
                    string_retained_bytes(field.as_str())?,
                    match asset_ref {
                        AssetRef::Path(path) => string_retained_bytes(path.as_str())?,
                        AssetRef::StableId(_) => 16,
                    },
                ],
            )?,
            ScenePatchOperation::RemoveEntity { entity } => string_retained_bytes(entity.as_str())?,
            ScenePatchOperation::RemoveComponent { entity, component } => checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    string_retained_bytes(entity.as_str())?,
                    string_retained_bytes(component.as_str())?,
                ],
            )?,
            ScenePatchOperation::RemoveField {
                entity,
                component,
                field,
                ..
            } => checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    string_retained_bytes(entity.as_str())?,
                    string_retained_bytes(component.as_str())?,
                    string_retained_bytes(field.as_str())?,
                ],
            )?,
            ScenePatchOperation::Reparent { entity, parent } => checked_plan_sum(
                ProjectContentBudgetKind::RetainedBytes,
                [
                    string_retained_bytes(entity.as_str())?,
                    parent
                        .as_ref()
                        .map(|parent| string_retained_bytes(parent.as_str()))
                        .transpose()?
                        .unwrap_or(0),
                ],
            )?,
        };
        checked_add_to(&mut total, dynamic, ProjectContentBudgetKind::RetainedBytes)?;
    }
    Ok(total)
}

fn component_value_retained_bytes(
    value: &nara_reflect::ComponentValue,
) -> Result<usize, ProjectContentError> {
    let cost = value.cost();
    checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [
            cost.logical_bytes(),
            checked_plan_product(
                ProjectContentBudgetKind::RetainedBytes,
                cost.nodes(),
                VALUE_NODE_ALLOCATION_BYTES,
            )?,
        ],
    )
}

fn string_retained_bytes(value: &str) -> Result<usize, ProjectContentError> {
    value
        .len()
        .checked_add(STRING_ALLOCATION_OVERHEAD)
        .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::RetainedBytes))
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
) -> Result<usize, ProjectContentError> {
    let mut total = checked_plan_sum(
        ProjectContentBudgetKind::RetainedBytes,
        [
            size_of::<ProjectContentSnapshotInner>(),
            ARC_CONTROL_BLOCK_BYTES,
            2 * ARC_CONTROL_BLOCK_BYTES,
            SNAPSHOT_LEASE_OVERHEAD,
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
        ],
    )?;
    for path in prefabs.keys() {
        checked_add_to(
            &mut total,
            string_retained_bytes(path.as_str())?,
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
    use nara_scene::{ScenePatchDocument, ScenePatchOperation};

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
            ScenePatchOperation::Reparent {
                entity: entity.clone(),
                parent: Some(parent.clone()),
            },
            ScenePatchOperation::Reparent {
                entity: entity.clone(),
                parent: None,
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
            ],
        )
        .unwrap();

        assert_eq!(patch_retained_bytes(&patch).unwrap(), expected);
    }
}
