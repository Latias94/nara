use std::{marker::PhantomData, sync::Arc};

use nara_asset::{
    AssetMeta, AssetMetaFileLimits, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
    AssetSourceKind, SourceAssetImporter,
};
use nara_fs::{ContentDigest, FileIdentity, RelativePath};
use nara_tooling::{
    EditorSourceAssetDomainView, EditorSourceAssetImportAction,
    EditorSourceAssetImportActionOutcome, EditorSourceAssetImportAttempt,
    EditorSourceAssetImportRejection as Rejection, EditorSourceAssetImportView,
    EditorSourceAssetMetadataView,
};
use serde::Serialize;

use crate::project_host::{
    authoring::{AuthoringProjectSessionSnapshot, ProjectSourceIndexSnapshot, ProjectSourceKind},
    persistence::FilePublicationHost,
};
use crate::{
    ProductRecipeError,
    project_content::{
        ProjectContentBudgetHost, ProjectContentError, validate_asset_domain_kind,
        validate_editor_source,
    },
};

use super::{EditorProjectSession, asset_metadata::EditorAssetMetadataSnapshot};

mod work;
use work::{PendingSourceImport, SourceImportWork};

/// A source-backed metadata authoring facet explicitly compiled into the Editor.
///
/// Preparation validates sources through the same domain as Check/Play. Its temporary CPU
/// product is discarded; successful import publishes only canonical metadata. This is not an
/// Editor resource cache, native/GPU importer, or project-code loader. Defaults and settings
/// codecs must be deterministic and effect-free.
pub trait EditorSourceAssetDomain: Send + Sync + 'static {
    /// The source domain reused for metadata, source preparation and resource limits.
    type Domain: SourceAssetImporter<Settings: Serialize>;

    fn default_settings() -> <Self::Domain as SourceAssetImporter>::Settings;

    /// Validates an authored transition from the current metadata to its replacement.
    ///
    /// The Host admits the previous settings from the exact metadata leaf fenced by publication.
    /// This runs before source preparation for Reimport only, under the same work budget and panic
    /// containment. Domains with authored continuity must reject lost or reused lineage here;
    /// standalone candidate semantics remain the source importer's responsibility. Validation
    /// must be deterministic and effect-free, and cannot establish history beyond that leaf.
    /// Each typed metadata admission must fit the domain's declared work allowance; the Host
    /// reserves two allowances while both settings are live.
    fn validate_settings_successor(
        previous: &<Self::Domain as SourceAssetImporter>::Settings,
        candidate: &<Self::Domain as SourceAssetImporter>::Settings,
    ) -> Result<(), nara_diagnostic::Diagnostic> {
        let _ = (previous, candidate);
        Ok(())
    }

    /// Encodes a typed request for this facet's domain. The Host still admits the displayed action.
    fn import_action(
        relative_path: impl Into<Arc<str>>,
        settings: &<Self::Domain as SourceAssetImporter>::Settings,
    ) -> Result<EditorSourceAssetImportAction, Rejection> {
        let settings = serde_json::to_string(settings).map_err(|_| Rejection::InvalidSettings)?;
        Ok(EditorSourceAssetImportAction {
            relative_path: relative_path.into(),
            source_kind: Self::Domain::SOURCE_KIND.into(),
            settings: settings.into(),
        })
    }
}

/// Immutable compiled contributor selection, independent of game recipe and project files.
#[derive(Clone, Default)]
pub struct EditorSourceAssetImporters {
    providers: Vec<ImporterEntry>,
}

#[derive(Clone)]
struct ImporterEntry {
    provider: Arc<dyn SourceImporter>,
    view: Option<EditorSourceAssetDomainView>,
}

impl EditorSourceAssetImporters {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn add<D: EditorSourceAssetDomain>(&mut self) -> Result<(), ProductRecipeError> {
        validate_asset_domain_kind(
            D::Domain::SOURCE_KIND,
            self.providers
                .iter()
                .any(|entry| entry.provider.kind() == D::Domain::SOURCE_KIND),
        )?;
        let provider: Arc<dyn SourceImporter> = Arc::new(TypedImporter::<D>(PhantomData));
        let view =
            provider
                .default_settings()
                .map(|default_settings| EditorSourceAssetDomainView {
                    source_kind: provider.kind(),
                    default_settings,
                });
        self.providers.push(ImporterEntry { provider, view });
        Ok(())
    }

    fn provider(&self, kind: &str) -> Option<Arc<dyn SourceImporter>> {
        self.providers
            .iter()
            .find(|entry| entry.provider.kind() == kind)
            .map(|entry| Arc::clone(&entry.provider))
    }
}

trait SourceImporter: Send + Sync {
    fn kind(&self) -> &'static str;
    fn default_settings(&self) -> Option<Arc<str>>;
    fn settings(&self, metadata: &AssetMetaIdentityCandidate) -> Option<Arc<str>>;
    fn metadata(&self, identity: AssetMetaIdentity, settings: &str) -> Result<String, Rejection>;
    fn validate(
        &self,
        assets: &nara_fs::DirectoryCapability,
        metadata: &AssetMetaIdentityCandidate,
        expected_source: FileIdentity,
        budget: &ProjectContentBudgetHost,
        previous: Option<&AssetMetaIdentityCandidate>,
    ) -> Result<crate::project_content::EditorSourceValidation, ProjectContentError>;
}

struct TypedImporter<D>(PhantomData<fn() -> D>);

impl<D: EditorSourceAssetDomain> SourceImporter for TypedImporter<D> {
    fn kind(&self) -> &'static str {
        D::Domain::SOURCE_KIND
    }
    fn default_settings(&self) -> Option<Arc<str>> {
        serde_json::to_string(&D::default_settings())
            .ok()
            .map(Arc::from)
    }
    fn settings(&self, metadata: &AssetMetaIdentityCandidate) -> Option<Arc<str>> {
        let candidate = metadata
            .admit::<<D::Domain as SourceAssetImporter>::Settings>()
            .ok()?;
        serde_json::to_string(candidate.settings())
            .ok()
            .map(Arc::from)
    }
    fn metadata(&self, identity: AssetMetaIdentity, settings: &str) -> Result<String, Rejection> {
        let settings: <D::Domain as SourceAssetImporter>::Settings =
            serde_json::from_str(settings).map_err(|_| Rejection::InvalidSettings)?;
        AssetMeta::new(identity, settings)
            .to_json_string()
            .map_err(|_| Rejection::InvalidSettings)
    }
    fn validate(
        &self,
        assets: &nara_fs::DirectoryCapability,
        metadata: &AssetMetaIdentityCandidate,
        expected_source: FileIdentity,
        budget: &ProjectContentBudgetHost,
        previous: Option<&AssetMetaIdentityCandidate>,
    ) -> Result<crate::project_content::EditorSourceValidation, ProjectContentError> {
        validate_editor_source::<D::Domain>(
            assets,
            metadata,
            expected_source,
            budget,
            previous,
            || {
                let Some(previous) = previous else {
                    return Ok(());
                };
                let settings_error = |_| {
                    source_import_error(
                        "project.editor.source-import-settings-invalid",
                        "The asset metadata settings could not be admitted",
                    )
                };
                let previous = previous
                    .admit::<<D::Domain as SourceAssetImporter>::Settings>()
                    .map_err(settings_error)?;
                let candidate = metadata
                    .admit::<<D::Domain as SourceAssetImporter>::Settings>()
                    .map_err(settings_error)?;
                D::validate_settings_successor(previous.settings(), candidate.settings())
                    .map_err(ProjectContentError::asset_domain)
            },
        )
    }
}

pub(super) struct EditorSourceAssetImportHost {
    pub(super) importers: EditorSourceAssetImporters,
    pending: Option<PendingSourceImport>,
    latest: Option<(Arc<str>, EditorSourceAssetImportAttempt)>,
    budget: ProjectContentBudgetHost,
    projection: Option<(
        Arc<EditorAssetMetadataSnapshot>,
        AssetPath,
        EditorSourceAssetMetadataView,
    )>,
}

impl Default for EditorSourceAssetImportHost {
    fn default() -> Self {
        Self {
            importers: EditorSourceAssetImporters::default(),
            pending: None,
            latest: None,
            budget: ProjectContentBudgetHost::new(Default::default()),
            projection: None,
        }
    }
}

impl EditorSourceAssetImportHost {
    /// Cancelled work remains in the same pending slot through physical return and file cleanup.
    pub(super) fn schema_adoption_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn has_pending_work(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn target(&self) -> Option<&str> {
        self.latest.as_ref().map(|(target, _)| target.as_ref())
    }

    pub(super) fn cancel(&mut self) {
        if let Some(pending) = &self.pending {
            pending.cancel();
        }
    }

    fn poll(&mut self) -> Option<EditorSourceAssetImportAttempt> {
        let outcome = self.pending.as_mut()?.poll()?;
        self.pending = None;
        if let Some((_, latest)) = &mut self.latest {
            *latest = outcome.clone();
        }
        Some(outcome)
    }

    pub(super) fn projection(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        index: &Arc<ProjectSourceIndexSnapshot>,
        metadata: &Arc<EditorAssetMetadataSnapshot>,
        path: &str,
    ) -> Option<EditorSourceAssetImportView> {
        let asset_path = asset_path(project, index, path)?;
        if self.importers.providers.is_empty() || !metadata.matches_source_index(index) {
            return None;
        }
        let status = if let Some((basis, path, status)) = &self.projection
            && Arc::ptr_eq(basis, metadata)
            && *path == asset_path
        {
            status.clone()
        } else if let Some(stable_id) = metadata.conflict(&asset_path) {
            EditorSourceAssetMetadataView::IdentityConflict { stable_id }
        } else if let Some(candidate) = metadata.candidate(&asset_path) {
            let AssetSourceKind::Other(kind) = candidate.identity().source_kind() else {
                return None;
            };
            EditorSourceAssetMetadataView::Ready {
                stable_id: candidate.identity().stable_id(),
                source_kind: Arc::from(kind.as_str()),
                settings: self
                    .importers
                    .provider(kind)
                    .and_then(|provider| provider.settings(candidate)),
            }
        } else if index.entry(&format!("{path}.meta")).is_some() {
            EditorSourceAssetMetadataView::Invalid
        } else {
            EditorSourceAssetMetadataView::Missing
        };
        self.projection = Some((Arc::clone(metadata), asset_path, status.clone()));
        let domains = self
            .importers
            .providers
            .iter()
            .filter_map(|entry| entry.view.clone())
            .collect();
        Some(EditorSourceAssetImportView {
            relative_path: Arc::from(path),
            domains,
            metadata: status,
            attempt: self
                .latest
                .as_ref()
                .filter(|(target, _)| target.as_ref() == path)
                .map_or(EditorSourceAssetImportAttempt::Idle, |(_, attempt)| {
                    attempt.clone()
                }),
        })
    }
}

fn asset_path(
    project: &AuthoringProjectSessionSnapshot,
    index: &ProjectSourceIndexSnapshot,
    path: &str,
) -> Option<AssetPath> {
    let entry = index.entry(path)?;
    if !matches!(
        entry.kind(),
        ProjectSourceKind::AssetSource | ProjectSourceKind::Unknown
    ) || entry.scoped_identity().is_none()
    {
        return None;
    }
    let prefix = project.settings().paths.assets.as_str();
    let suffix = path.strip_prefix(prefix)?.strip_prefix('/')?;
    AssetPath::new(suffix).ok()
}

impl EditorProjectSession {
    pub(super) fn dispatch_source_asset_import(
        &mut self,
        index: Option<&Arc<ProjectSourceIndexSnapshot>>,
        action: EditorSourceAssetImportAction,
    ) -> EditorSourceAssetImportActionOutcome {
        let result = self.begin_source_asset_import(index, action);
        result.map_or_else(EditorSourceAssetImportActionOutcome::Rejected, |()| {
            EditorSourceAssetImportActionOutcome::Started
        })
    }

    fn begin_source_asset_import(
        &mut self,
        index: Option<&Arc<ProjectSourceIndexSnapshot>>,
        action: EditorSourceAssetImportAction,
    ) -> Result<(), Rejection> {
        if self.closing
            || self.schema_adoption_fenced()
            || self.source_tools.reclamation_fenced()
            || self.source_asset_import.has_pending_work()
            || matches!(
                self.image_import.state(),
                super::image_import::EditorImageImportState::Pending { .. }
            )
        {
            return Err(Rejection::Busy);
        }
        let index = index.ok_or(Rejection::Unavailable)?;
        let project = self.authoring_project().ok_or(Rejection::Unavailable)?;
        if !project
            .source_index_snapshot()
            .is_some_and(|current| Arc::ptr_eq(&current, index))
        {
            return Err(Rejection::StaleSource);
        }
        let catalog = self
            .image_import
            .metadata_snapshot(index)
            .ok_or(Rejection::Unavailable)?;
        let path =
            asset_path(project, index, &action.relative_path).ok_or(Rejection::Unavailable)?;
        if catalog.conflict(&path).is_some() {
            return Err(Rejection::InvalidMetadata);
        }
        let provider = self
            .source_asset_import
            .importers
            .provider(&action.source_kind)
            .ok_or(Rejection::Unavailable)?;
        let metadata_path = format!("{}.meta", action.relative_path);
        let root = project.project_root();
        let byte_limit = AssetMetaFileLimits::default().encoded_bytes().get();
        if action.settings.len() > byte_limit {
            return Err(Rejection::InvalidSettings);
        }
        let (stable_id, publication, previous) = match catalog.candidate(&path) {
            Some(candidate) => {
                if candidate.identity().source_kind()
                    != &AssetSourceKind::Other(provider.kind().to_owned())
                {
                    return Err(Rejection::InvalidMetadata);
                }
                let digest = catalog
                    .observed_digest(&path)
                    .ok_or(Rejection::InvalidMetadata)?;
                // The catalog is discovery data. Establish a new admission from a live leaf;
                // its historical native identity cannot prove continuous object custody.
                let relative = RelativePath::new(&metadata_path)
                    .map_err(|_| Rejection::PublicationUnavailable)?;
                let file = root
                    .open_file_exact(&relative)
                    .map_err(|_| Rejection::PublicationUnavailable)?;
                let bytes = file
                    .read_to_end_bounded(byte_limit as u64)
                    .map_err(|_| Rejection::PublicationUnavailable)?;
                if ContentDigest::of_bytes(&bytes) != digest {
                    return Err(Rejection::PublicationUnavailable);
                }
                let current = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
                    bytes.into_boxed_slice(),
                    AssetMetaFileLimits::default(),
                )
                .map_err(|_| Rejection::InvalidMetadata)?;
                if current.identity().path() != &path
                    || current.identity().source_kind() != candidate.identity().source_kind()
                    || current.identity().stable_id() != candidate.identity().stable_id()
                {
                    return Err(Rejection::InvalidMetadata);
                }
                let publication = FilePublicationHost::bind_existing(
                    &root,
                    &metadata_path,
                    &file,
                    digest,
                    byte_limit as u64,
                )
                .map_err(|_| Rejection::PublicationUnavailable)?;
                (current.identity().stable_id(), publication, Some(current))
            }
            None => {
                if index.entry(&metadata_path).is_some() {
                    return Err(Rejection::InvalidMetadata);
                }
                let stable_id = catalog
                    .fresh_stable_id()
                    .ok_or(Rejection::PublicationUnavailable)?;
                let publication =
                    FilePublicationHost::bind_missing(&root, &metadata_path, byte_limit as u64)
                        .map_err(|_| Rejection::PublicationUnavailable)?;
                (stable_id, publication, None)
            }
        };
        let encoded = provider.metadata(
            AssetMetaIdentity::new(
                stable_id,
                path,
                AssetSourceKind::Other(provider.kind().to_owned()),
            ),
            &action.settings,
        )?;
        if encoded.len() > byte_limit {
            return Err(Rejection::InvalidSettings);
        }
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned(
            encoded.clone().into_bytes().into_boxed_slice(),
        )
        .map_err(|_| Rejection::InvalidSettings)?;
        let expected_source = index
            .entry(&action.relative_path)
            .and_then(|entry| entry.scoped_identity())
            .ok_or(Rejection::StaleSource)?;
        let assets_path = RelativePath::new(project.settings().paths.assets.as_str())
            .map_err(|_| Rejection::Unavailable)?;
        let assets = root
            .open_directory(&assets_path)
            .map_err(|_| Rejection::Unavailable)?;
        let publication = self
            .authoring_runtime
            .host_custody()
            .retain_publication(publication)
            .ok_or(Rejection::Busy)?;
        let pending = PendingSourceImport::start(
            self.authoring_runtime.persistence_tasks(),
            publication,
            SourceImportWork {
                provider,
                assets,
                metadata,
                previous,
                encoded,
                expected_source,
                budget: self.source_asset_import.budget.clone(),
                #[cfg(test)]
                gate: None,
            },
        )?;
        self.source_asset_import.latest = Some((
            Arc::clone(&action.relative_path),
            EditorSourceAssetImportAttempt::Preparing,
        ));
        self.source_asset_import.pending = Some(pending);
        Ok(())
    }

    pub(super) fn advance_source_asset_import(&mut self) {
        let Some(outcome) = self.source_asset_import.poll() else {
            return;
        };
        if matches!(
            outcome,
            EditorSourceAssetImportAttempt::MetadataPublished { .. }
                | EditorSourceAssetImportAttempt::ReconcileRequired
        ) {
            let scheduled = match &mut self.execution {
                super::EditorExecutionLane::Authoring(project) => {
                    self.image_import.schedule_refresh(project).is_some()
                }
                super::EditorExecutionLane::TrustedCompiled(_) => false,
            };
            if !scheduled && let Some((_, latest)) = &mut self.source_asset_import.latest {
                *latest = EditorSourceAssetImportAttempt::ReconcileRequired;
            }
        }
    }

    /// Cancels and observes only admitted import work. A returned publication can reserve a
    /// source-index refresh, but this method never starts a scan or another import worker.
    pub(super) fn drain_schema_adoption_source_asset_import(&mut self) {
        self.source_asset_import.cancel();
        self.advance_source_asset_import();
    }
}

fn failed(code: &'static str, summary: &'static str) -> EditorSourceAssetImportAttempt {
    let error = source_import_error(code, summary);
    EditorSourceAssetImportAttempt::Failed {
        diagnostics: error.diagnostics().clone(),
    }
}

fn source_import_error(code: &'static str, summary: &'static str) -> ProjectContentError {
    ProjectContentError::asset_domain(nara_diagnostic::Diagnostic::error(
        nara_diagnostic::DiagnosticCode::new(code).expect("static source import diagnostic code"),
        nara_diagnostic::SafeSummary::new(summary)
            .expect("static source import diagnostic summary"),
    ))
}
