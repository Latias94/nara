use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use nara_asset::{
    AssetId, AssetMeta, AssetMetaCandidate, AssetMetaIdentity, AssetPath, AssetRecord,
    AssetSourceKind, AssetVersion, SourceHash, StableAssetId,
};
use nara_fs::{ContentDigest, FileIdentity, RelativePath};
use nara_image::ImageImportSettings;
use nara_tasks::{TaskReturnObserver, TaskReturnState};
use nara_tilemap::{TileSetSource, TileSetSourceFileLimits};

#[cfg(test)]
use crate::project_host::persistence::CleanupTestControl;
use crate::project_host::{
    authoring::{
        AuthoringDocumentCapture, AuthoringProjectSessionSnapshot, ProjectSourceIndexGeneration,
        ProjectSourceIndexSnapshot,
    },
    persistence::{
        FilePublicationBinding, FilePublicationFailureStage, FilePublicationHost,
        FilePublicationOutcome, FilePublicationRejection,
    },
};

use super::catalog::{
    EditorImageCatalogAttempt, EditorImageCatalogBuildError, EditorImageCatalogSnapshot,
    EditorImageMetadataStatus,
};
use super::hydration::{EditorImageHydrationFailure, EditorImageHydrationHost};
use super::metadata_byte_limit;
use super::store::{EditorImageLastGood, EditorImageLastGoodStore, EditorImageSourceProof};
use crate::project_host::runtime::editor::asset_metadata::EditorAssetMetadataSnapshot;
#[cfg(feature = "runtime-ui")]
use crate::project_host::runtime::editor::authoring_images;
use crate::project_host::runtime::editor::authoring_runtime::{
    EditorAuthoringRuntime, EditorImageRuntimeFailure, EditorImageRuntimeTask,
    EditorImageRuntimeTerminal, EditorImageSourceCompletion, EditorTileSetSourceUpdate,
};
use crate::project_host::runtime::editor::persistence::RetainedHost;

mod publication;
use publication::MetadataPublication;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EditorImageImportGeneration(u64);

impl EditorImageImportGeneration {
    #[must_use]
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorImageImportFailure {
    CatalogUnavailable,
    CatalogRejected,
    Busy,
    ReconcileRequired,
    StaleFrame,
    EntryUnavailable,
    MetadataInvalid,
    StableIdUnavailable,
    LastGoodLimitExceeded,
    SourceUnavailable,
    SourceChanged,
    MetadataEncoding,
    MetadataPublicationRejected,
    MetadataPublicationFailed,
    MetadataPublicationUncertain,
    Runtime(EditorImageRuntimeFailure),
    ResultMismatch,
    CleanupPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorImageImportRequestResult {
    Started(EditorImageImportGeneration),
    Completed(EditorImageImportState),
    Rejected(EditorImageImportFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorImageImportState {
    Idle,
    Pending {
        generation: EditorImageImportGeneration,
    },
    Succeeded {
        generation: EditorImageImportGeneration,
        stable_id: StableAssetId,
        version: AssetVersion,
    },
    Failed {
        generation: EditorImageImportGeneration,
        failure: EditorImageImportFailure,
    },
    ReconcileRequired {
        generation: EditorImageImportGeneration,
    },
}

struct PendingImageImport {
    generation: EditorImageImportGeneration,
    project_root_identity: FileIdentity,
    relative_path: Arc<str>,
    asset_path: AssetPath,
    stable_id: StableAssetId,
    source_identity: FileIdentity,
    metadata: RetainedHost<FilePublicationHost>,
    settings: ImageImportSettings,
    task: EditorImageRuntimeTask,
}

#[derive(Clone, Copy)]
struct RefreshObligation {
    target_generation: ProjectSourceIndexGeneration,
}

#[derive(Clone, Copy)]
struct ReconciliationObligation {
    target_generation: u64,
}

#[derive(Clone, Copy)]
pub(super) enum EditorImageImportLatestMetadata {
    Ready,
    Uncertain,
}

#[derive(Clone)]
pub(super) struct EditorImageImportLatestDetail {
    relative_path: Arc<str>,
    detail_revision: u64,
    asset_path: AssetPath,
    stable_id: StableAssetId,
    settings: ImageImportSettings,
    metadata: EditorImageImportLatestMetadata,
}

#[derive(Clone)]
struct EditorImageImportLatestAttempt {
    target: Arc<str>,
    state: EditorImageImportState,
    detail: Option<EditorImageImportLatestDetail>,
}

impl EditorImageImportLatestDetail {
    #[must_use]
    pub(super) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub(super) const fn detail_revision(&self) -> u64 {
        self.detail_revision
    }

    #[must_use]
    pub(super) const fn asset_path(&self) -> &AssetPath {
        &self.asset_path
    }

    #[must_use]
    pub(super) const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub(super) const fn settings(&self) -> ImageImportSettings {
        self.settings
    }

    #[must_use]
    pub(super) const fn metadata(&self) -> EditorImageImportLatestMetadata {
        self.metadata
    }
}

#[derive(Clone)]
pub(crate) struct EditorImageImportFrame {
    relative_path: Arc<str>,
    detail_revision: u64,
    source_index: Arc<ProjectSourceIndexSnapshot>,
    catalog: Arc<EditorImageCatalogSnapshot>,
}

impl EditorImageImportFrame {
    #[must_use]
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub(crate) const fn detail_revision(&self) -> u64 {
        self.detail_revision
    }

    #[must_use]
    pub(super) fn source_index_generation(&self) -> ProjectSourceIndexGeneration {
        self.source_index.generation()
    }

    #[must_use]
    pub(super) fn entry(&self) -> Option<&super::catalog::EditorImageCatalogEntry> {
        self.catalog.entry(&self.relative_path)
    }

    #[must_use]
    pub(super) fn contains_stable_id(&self, stable_id: StableAssetId) -> bool {
        self.catalog.contains_stable_id(stable_id)
    }

    #[must_use]
    fn matches_action(&self, relative_path: &str, expected_detail_revision: u64) -> bool {
        self.relative_path.as_ref() == relative_path
            && self.detail_revision == expected_detail_revision
    }
}

pub(crate) struct EditorImageImportHost {
    catalog_basis: Option<Arc<ProjectSourceIndexSnapshot>>,
    catalog_attempt: Option<EditorImageCatalogAttempt>,
    catalog: Option<Arc<EditorImageCatalogSnapshot>>,
    tile_source_refresh: Option<VecDeque<AssetRecord>>,
    catalog_failure: Option<EditorImageCatalogBuildError>,
    runtime_revocation_failure: Option<EditorImageRuntimeFailure>,
    next_attempt_generation: u64,
    pending: Option<PendingImageImport>,
    retiring_imports: Vec<TaskReturnObserver>,
    latest: Option<EditorImageImportLatestAttempt>,
    retained_details: BTreeMap<Arc<str>, EditorImageImportLatestDetail>,
    last_good: EditorImageLastGoodStore,
    hydration: EditorImageHydrationHost,
    refresh: Option<RefreshObligation>,
    reconciliation: Option<ReconciliationObligation>,
    publication_cleanup: Option<RetainedHost<FilePublicationHost>>,
    #[cfg(test)]
    reject_next_valid_metadata_receipt: bool,
    #[cfg(test)]
    fail_next_metadata_after_write: Option<CleanupTestControl>,
    #[cfg(test)]
    panic_after_metadata_publication: bool,
}

impl Default for EditorImageImportHost {
    fn default() -> Self {
        Self {
            catalog_basis: None,
            catalog_attempt: None,
            catalog: None,
            tile_source_refresh: None,
            catalog_failure: None,
            runtime_revocation_failure: None,
            next_attempt_generation: 1,
            pending: None,
            retiring_imports: Vec::new(),
            latest: None,
            retained_details: BTreeMap::new(),
            last_good: EditorImageLastGoodStore::default(),
            hydration: EditorImageHydrationHost::default(),
            refresh: None,
            reconciliation: None,
            publication_cleanup: None,
            #[cfg(test)]
            reject_next_valid_metadata_receipt: false,
            #[cfg(test)]
            fail_next_metadata_after_write: None,
            #[cfg(test)]
            panic_after_metadata_publication: false,
        }
    }
}

impl EditorImageImportHost {
    #[must_use]
    pub(crate) fn metadata_snapshot(
        &self,
        source_index: &Arc<ProjectSourceIndexSnapshot>,
    ) -> Option<Arc<EditorAssetMetadataSnapshot>> {
        if self.refresh.is_some() || self.reconciliation.is_some() {
            return None;
        }
        self.catalog
            .as_ref()
            .filter(|catalog| catalog.matches_source_index(source_index))
            .map(|catalog| Arc::clone(&catalog.metadata))
    }

    #[must_use]
    pub(crate) fn frame_for(
        &self,
        source_index: &Arc<ProjectSourceIndexSnapshot>,
        relative_path: &str,
    ) -> Option<EditorImageImportFrame> {
        let catalog = self
            .catalog
            .as_ref()
            .filter(|catalog| catalog.matches_source_index(source_index))
            .map(Arc::clone)?;
        catalog.entry(relative_path)?;
        Some(EditorImageImportFrame {
            relative_path: Arc::from(relative_path),
            detail_revision: source_index.generation().get(),
            source_index: Arc::clone(source_index),
            catalog,
        })
    }

    #[must_use]
    pub(crate) fn state(&self) -> EditorImageImportState {
        self.latest
            .as_ref()
            .map_or(EditorImageImportState::Idle, |latest| latest.state)
    }

    #[must_use]
    pub(super) fn attempt_for(&self, relative_path: &str) -> EditorImageImportState {
        self.latest
            .as_ref()
            .filter(|latest| latest.target.as_ref() == relative_path)
            .map_or(EditorImageImportState::Idle, |latest| latest.state)
    }

    #[must_use]
    pub(super) fn detail_for(&self, relative_path: &str) -> Option<&EditorImageImportLatestDetail> {
        self.latest
            .as_ref()
            .filter(|latest| latest.target.as_ref() == relative_path)
            .and_then(|latest| latest.detail.as_ref())
            .or_else(|| self.retained_details.get(relative_path))
    }

    #[must_use]
    pub(crate) fn last_good(&self, asset_path: &AssetPath) -> Option<&EditorImageLastGood> {
        self.last_good.get(asset_path)
    }

    #[must_use]
    pub(crate) fn authoring_image_asset_ids(&self) -> BTreeSet<AssetId> {
        self.last_good.asset_ids()
    }

    pub(crate) fn initialize_authoring_hydration(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) {
        self.hydration.initialize(project, &mut self.last_good);
    }

    pub(crate) fn enqueue_authoring_capture(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        capture: &AuthoringDocumentCapture,
    ) {
        self.hydration
            .enqueue_authoring_capture(project, capture, &mut self.last_good);
    }

    #[must_use]
    pub(crate) const fn last_good_store_changed(&self) -> bool {
        self.last_good.changed()
    }

    pub(crate) fn acknowledge_last_good_store_changed(&mut self) {
        self.last_good.acknowledge_changed();
    }

    #[must_use]
    pub(crate) fn take_hydration_failure(&mut self) -> Option<EditorImageHydrationFailure> {
        self.hydration.take_failure()
    }

    #[must_use]
    pub(crate) fn has_pending_work(&self) -> bool {
        self.catalog_attempt.is_some()
            || self.tile_source_refresh.is_some()
            || self.pending.is_some()
            || !self.retiring_imports.is_empty()
            || self.refresh.is_some()
            || self.hydration.has_pending_work()
    }

    /// Observes admitted work and cleanup, including cancelled workers awaiting physical return.
    /// Queued catalog, refresh and hydration inputs cannot run while the caller fences `advance`.
    #[must_use]
    pub(in crate::project_host::runtime::editor) fn schema_adoption_pending(&self) -> bool {
        self.schema_adoption_import_pending() || self.hydration.schema_adoption_pending()
    }

    pub(in crate::project_host::runtime::editor) fn schema_adoption_import_pending(&self) -> bool {
        self.pending.is_some()
            || !self.retiring_imports.is_empty()
            || self.publication_cleanup.is_some()
    }

    /// Cancels admitted image work and observes its return without admitting another worker.
    pub(in crate::project_host::runtime::editor) fn advance_schema_adoption_drain(&mut self) {
        if let Some(generation) = self.pending.as_ref().map(|pending| pending.generation) {
            self.cancel_import_pending();
            self.set_latest_state(EditorImageImportState::Failed {
                generation,
                failure: EditorImageImportFailure::Runtime(
                    EditorImageRuntimeFailure::TaskCancelled,
                ),
            });
        }
        self.hydration.observe_schema_adoption_drain();
        self.observe_retiring_imports();
        self.retry_publication_cleanup();
    }

    fn observe_retiring_imports(&mut self) {
        self.retiring_imports
            .retain(|observer| observer.state() == TaskReturnState::Pending);
    }

    fn retain_import_return(&mut self, task: &EditorImageRuntimeTask) {
        let observer = task.return_observer();
        if observer.state() == TaskReturnState::Pending {
            self.retiring_imports.push(observer);
        }
    }

    #[must_use]
    pub(crate) fn request_blocker(&self) -> Option<EditorImageImportFailure> {
        if let Some(failure) = self.runtime_revocation_failure {
            Some(EditorImageImportFailure::Runtime(failure))
        } else if self.publication_cleanup.is_some() {
            Some(EditorImageImportFailure::CleanupPending)
        } else if self.pending.is_some() {
            Some(EditorImageImportFailure::Busy)
        } else if self.reconciliation.is_some() {
            Some(EditorImageImportFailure::ReconcileRequired)
        } else if self.refresh.is_some() {
            Some(EditorImageImportFailure::StaleFrame)
        } else {
            None
        }
    }

    pub(crate) fn cancel_pending(&mut self) {
        self.cancel_import_pending();
        self.hydration.cancel_pending();
    }

    fn cancel_import_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.retain_import_return(&pending.task);
            self.retire_metadata(pending.metadata);
        }
    }

    pub(crate) fn retry_publication_cleanup(&mut self) -> bool {
        let Some(publication) = self.publication_cleanup.as_ref() else {
            return true;
        };
        if !publication.retire() {
            return false;
        }
        self.publication_cleanup = None;
        true
    }

    #[cfg(test)]
    pub(crate) fn test_reject_next_valid_metadata_receipt(&mut self) {
        self.reject_next_valid_metadata_receipt = true;
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_metadata_after_write(&mut self) -> CleanupTestControl {
        let control = CleanupTestControl::blocked();
        self.fail_next_metadata_after_write = Some(control.clone());
        control
    }

    #[cfg(test)]
    pub(crate) fn test_panic_after_metadata_publication(&mut self) {
        self.panic_after_metadata_publication = true;
    }

    #[cfg(test)]
    pub(crate) fn test_allow_publication_cleanup(&mut self) {
        self.publication_cleanup
            .as_ref()
            .expect("the test requires retained metadata cleanup")
            .try_with(FilePublicationHost::test_allow_cleanup)
            .expect("the retained publication Host is available");
    }

    pub(crate) fn advance(
        &mut self,
        project: &mut AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
    ) -> Option<EditorImageRuntimeFailure> {
        self.retry_publication_cleanup();
        self.observe_retiring_imports();
        let previous_failure = self.runtime_revocation_failure;
        self.advance_refresh(project);
        self.advance_catalog(project, runtime);
        self.advance_tile_sources(project, runtime);
        self.advance_reconciliation(project);
        self.advance_import(project, runtime);
        if self.runtime_revocation_failure.is_some() {
            self.hydration.cancel_pending();
        } else if self.pending.is_none() {
            self.hydration
                .advance(project, runtime, &mut self.last_good);
            if let Some(failure) = self.hydration.take_revocation_failure() {
                self.runtime_revocation_failure = Some(failure);
            }
        }
        if let Err(failure) = runtime.reconcile_tile_sets(Vec::new()) {
            self.runtime_revocation_failure = Some(failure);
        }
        if self.runtime_revocation_failure.is_some() {
            self.hydration.cancel_pending();
            if let Some(catalog) = &self.catalog {
                self.last_good.retain_catalog_eligible(|path, image| {
                    catalog.retains_imported_image(path, image.stable_id(), image.source_identity())
                });
            }
        }
        self.runtime_revocation_failure
            .filter(|_| previous_failure.is_none())
    }

    pub(crate) fn request(
        &mut self,
        project: &mut AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
        frame: EditorImageImportFrame,
        project_relative_path: &str,
        expected_detail_revision: u64,
        settings: ImageImportSettings,
    ) -> EditorImageImportRequestResult {
        self.retry_publication_cleanup();
        if let Some(failure) = self.request_blocker() {
            return EditorImageImportRequestResult::Rejected(failure);
        }
        if !frame.matches_action(project_relative_path, expected_detail_revision) {
            return EditorImageImportRequestResult::Rejected(EditorImageImportFailure::StaleFrame);
        }
        let EditorImageImportFrame {
            relative_path: _,
            detail_revision: _,
            source_index,
            catalog,
        } = frame;
        let Some(current_index) = project.source_index_snapshot() else {
            return EditorImageImportRequestResult::Rejected(
                EditorImageImportFailure::CatalogUnavailable,
            );
        };
        let Some(current_catalog) = self.catalog.as_ref() else {
            return EditorImageImportRequestResult::Rejected(
                self.catalog_failure
                    .map_or(EditorImageImportFailure::CatalogUnavailable, |_| {
                        EditorImageImportFailure::CatalogRejected
                    }),
            );
        };
        if !Arc::ptr_eq(&current_index, &source_index)
            || !Arc::ptr_eq(current_catalog, &catalog)
            || !catalog.matches_source_index(&source_index)
            || catalog.root_identity() != project.project_root().identity()
        {
            return EditorImageImportRequestResult::Rejected(EditorImageImportFailure::StaleFrame);
        }
        let Some(entry) = catalog.entry(project_relative_path).cloned() else {
            return EditorImageImportRequestResult::Rejected(
                EditorImageImportFailure::EntryUnavailable,
            );
        };
        let Some(source_identity) = entry.source_identity() else {
            return EditorImageImportRequestResult::Rejected(
                EditorImageImportFailure::SourceUnavailable,
            );
        };
        if !self.last_good.can_insert(entry.asset_path()) {
            return EditorImageImportRequestResult::Rejected(
                EditorImageImportFailure::LastGoodLimitExceeded,
            );
        }

        let generation = match self.allocate_attempt_generation() {
            Some(generation) => generation,
            None => {
                return EditorImageImportRequestResult::Rejected(
                    EditorImageImportFailure::StableIdUnavailable,
                );
            }
        };
        self.latest = Some(EditorImageImportLatestAttempt {
            target: Arc::from(project_relative_path),
            state: EditorImageImportState::Idle,
            detail: None,
        });
        let project_root = project.project_root();
        let source_path = match RelativePath::new(project_relative_path) {
            Ok(path) => path,
            Err(_) => {
                return self.fail_request(generation, EditorImageImportFailure::SourceUnavailable);
            }
        };
        let source = match project_root.open_file(&source_path) {
            Ok(source) if source.identity() == source_identity => source,
            Ok(_) => return self.fail_request(generation, EditorImageImportFailure::SourceChanged),
            Err(_) => {
                return self.fail_request(generation, EditorImageImportFailure::SourceUnavailable);
            }
        };

        let last_good_stable_id = self
            .last_good
            .get(entry.asset_path())
            .map(EditorImageLastGood::stable_id);
        let (stable_id, previous_settings, publication) = match entry.metadata() {
            EditorImageMetadataStatus::Missing => {
                let stable_id = match last_good_stable_id {
                    Some(stable_id) if catalog.contains_stable_id(stable_id) => {
                        return self
                            .fail_request(generation, EditorImageImportFailure::MetadataInvalid);
                    }
                    Some(stable_id) => stable_id,
                    None => {
                        let Some(stable_id) = fresh_stable_id(&catalog) else {
                            return self.fail_request(
                                generation,
                                EditorImageImportFailure::StableIdUnavailable,
                            );
                        };
                        stable_id
                    }
                };
                let publication = match FilePublicationHost::bind_missing(
                    &project_root,
                    entry.metadata_project_relative_path(),
                    metadata_byte_limit(),
                ) {
                    Ok(publication) => publication,
                    Err(_) => {
                        return self.fail_request(
                            generation,
                            EditorImageImportFailure::MetadataPublicationRejected,
                        );
                    }
                };
                (stable_id, None, publication)
            }
            EditorImageMetadataStatus::Ready(receipt) => {
                if last_good_stable_id.is_some_and(|stable_id| stable_id != receipt.stable_id()) {
                    return self
                        .fail_request(generation, EditorImageImportFailure::MetadataInvalid);
                }
                // A catalog receipt is a historical data observation. Admit the current
                // leaf afresh, and keep it alive until the publisher owns that same object.
                let metadata = match RelativePath::new(entry.metadata_project_relative_path())
                    .ok()
                    .and_then(|path| project_root.open_file(&path).ok())
                {
                    Some(metadata) => metadata,
                    None => {
                        return self.fail_request(
                            generation,
                            EditorImageImportFailure::MetadataPublicationRejected,
                        );
                    }
                };
                let bytes = match metadata.read_to_end_bounded(metadata_byte_limit()) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        return self.fail_request(
                            generation,
                            EditorImageImportFailure::MetadataPublicationRejected,
                        );
                    }
                };
                let admitted = AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(&bytes);
                let admitted = match admitted {
                    Ok(admitted)
                        if admitted.identity().path() == entry.asset_path()
                            && admitted.identity().source_kind() == &AssetSourceKind::Image
                            && admitted.identity().stable_id() == receipt.stable_id()
                            && *admitted.settings() == receipt.settings() =>
                    {
                        admitted
                    }
                    _ => {
                        return self
                            .fail_request(generation, EditorImageImportFailure::MetadataInvalid);
                    }
                };
                let digest = ContentDigest::of_bytes(&bytes);
                if digest != receipt.digest() {
                    return self.fail_request(
                        generation,
                        EditorImageImportFailure::MetadataPublicationRejected,
                    );
                }
                let publication = match FilePublicationHost::bind_existing(
                    &project_root,
                    entry.metadata_project_relative_path(),
                    &metadata,
                    digest,
                    metadata_byte_limit(),
                ) {
                    Ok(publication) => publication,
                    Err(_) => {
                        return self.fail_request(
                            generation,
                            EditorImageImportFailure::MetadataPublicationRejected,
                        );
                    }
                };
                (
                    admitted.identity().stable_id(),
                    Some(*admitted.settings()),
                    publication,
                )
            }
            EditorImageMetadataStatus::Invalid(_)
            | EditorImageMetadataStatus::IdentityConflict { .. } => {
                return self.fail_request(generation, EditorImageImportFailure::MetadataInvalid);
            }
        };

        #[cfg(feature = "runtime-ui")]
        if authoring_images::validate_authoring_image_choice(stable_id, entry.asset_path()).is_err()
        {
            return self.fail_request(generation, EditorImageImportFailure::CatalogRejected);
        }

        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                entry.asset_path().clone(),
                AssetSourceKind::Image,
            ),
            settings,
        );
        let encoded = match metadata.to_json_string() {
            Ok(encoded) => encoded,
            Err(_) => {
                return self.fail_request(generation, EditorImageImportFailure::MetadataEncoding);
            }
        };
        let metadata_changed = previous_settings != Some(settings);
        self.hydration.surrender_source(entry.asset_path());
        let mut detail_revision = source_index.generation().get();
        let metadata = if metadata_changed {
            match self.publish_metadata(runtime, publication, encoded.as_bytes()) {
                MetadataPublication::Published(publication) => {
                    let target_generation = self.schedule_refresh(project);
                    detail_revision = target_generation.map_or_else(
                        || source_index.generation().get().saturating_add(1),
                        ProjectSourceIndexGeneration::get,
                    );
                    if target_generation.is_none() {
                        self.remember_latest_detail(
                            project_relative_path,
                            detail_revision,
                            &entry,
                            stable_id,
                            settings,
                            EditorImageImportLatestMetadata::Ready,
                        );
                        self.require_reconciliation(generation, detail_revision);
                        return EditorImageImportRequestResult::Completed(self.state());
                    };
                    publication
                }
                MetadataPublication::Failed(failure) => {
                    return self.fail_request(generation, failure);
                }
                MetadataPublication::Uncertain => {
                    let target_generation = self
                        .schedule_refresh(project)
                        .map(ProjectSourceIndexGeneration::get)
                        .unwrap_or_else(|| source_index.generation().get().saturating_add(1));
                    self.remember_latest_detail(
                        project_relative_path,
                        target_generation,
                        &entry,
                        stable_id,
                        settings,
                        EditorImageImportLatestMetadata::Uncertain,
                    );
                    self.require_reconciliation(generation, target_generation);
                    return EditorImageImportRequestResult::Completed(self.state());
                }
            }
        } else {
            match self.retain_metadata(runtime, publication) {
                Ok(publication) => publication,
                Err(failure) => return self.fail_request(generation, failure),
            }
        };
        self.remember_latest_detail(
            project_relative_path,
            detail_revision,
            &entry,
            stable_id,
            settings,
            EditorImageImportLatestMetadata::Ready,
        );
        let record = AssetRecord::new(
            stable_id,
            entry.asset_path().clone(),
            AssetSourceKind::Image,
        );
        let task = match runtime.spawn_image_import(
            generation.get(),
            record,
            source,
            source_identity,
            project_root.share(),
            source_path.clone(),
            settings,
        ) {
            Ok(task) => task,
            Err(reason) => {
                return self.fail_request(generation, EditorImageImportFailure::Runtime(reason));
            }
        };

        self.pending = Some(PendingImageImport {
            generation,
            project_root_identity: project_root.identity(),
            relative_path: Arc::from(project_relative_path),
            asset_path: entry.asset_path().clone(),
            stable_id,
            source_identity,
            metadata,
            settings,
            task,
        });
        // The pending attempt now owns the complete registered Host. The independent
        // cleanup latch covered every earlier return and unwind before this transfer.
        self.publication_cleanup = None;
        self.set_latest_state(EditorImageImportState::Pending { generation });
        EditorImageImportRequestResult::Started(generation)
    }

    fn advance_catalog(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
    ) {
        if self.refresh.is_some() || project.source_index_failure().is_some() {
            self.tile_source_refresh = None;
            self.hydration.cancel_dependencies();
            self.catalog_basis = None;
            self.catalog_attempt = None;
            self.catalog = None;
            self.catalog_failure = None;
            return;
        }
        let Some(source_index) = project.source_index_snapshot() else {
            self.tile_source_refresh = None;
            self.hydration.cancel_dependencies();
            self.catalog_basis = None;
            self.catalog_attempt = None;
            self.catalog = None;
            // A refused successor refresh remains unavailable until a new index is observed.
            return;
        };
        if self
            .catalog
            .as_ref()
            .is_some_and(|catalog| catalog.matches_source_index(&source_index))
        {
            return;
        }
        if !self
            .catalog_basis
            .as_ref()
            .is_some_and(|basis| Arc::ptr_eq(basis, &source_index))
        {
            self.tile_source_refresh = None;
            self.hydration.cancel_dependencies();
            self.catalog = None;
            self.catalog_failure = None;
            self.catalog_attempt =
                match EditorImageCatalogAttempt::begin(project, Arc::clone(&source_index)) {
                    Ok(attempt) => Some(attempt),
                    Err(error) => {
                        self.catalog_failure = Some(error);
                        None
                    }
                };
            self.catalog_basis = Some(Arc::clone(&source_index));
        }
        let Some(attempt) = self.catalog_attempt.as_mut() else {
            return;
        };
        match attempt.advance(project) {
            Ok(Some(snapshot)) => {
                let snapshot = Arc::new(snapshot);
                if self.pending.as_ref().is_some_and(|pending| {
                    !snapshot.retains_imported_image(
                        &pending.asset_path,
                        pending.stable_id,
                        pending.source_identity,
                    )
                }) {
                    let pending = self
                        .pending
                        .take()
                        .expect("an ineligible import remains owned until cancellation");
                    self.set_latest_state(EditorImageImportState::Failed {
                        generation: pending.generation,
                        failure: EditorImageImportFailure::MetadataInvalid,
                    });
                    self.retain_import_return(&pending.task);
                    self.retire_metadata(pending.metadata);
                }
                let deferred = self
                    .hydration
                    .observe_catalog(Arc::clone(&snapshot), &self.last_good);
                let deferred = if self.runtime_revocation_failure.is_some() {
                    BTreeSet::new()
                } else {
                    deferred
                };
                let revoked = self.last_good.ineligible_handles(|path, image| {
                    deferred.contains(path)
                        || snapshot.retains_imported_image(
                            path,
                            image.stable_id(),
                            image.source_identity(),
                        )
                });
                if let Err(failure) = runtime.revoke_images(revoked) {
                    self.runtime_revocation_failure = Some(failure);
                }
                self.last_good.retain_catalog_eligible(|asset_path, image| {
                    deferred.contains(asset_path)
                        || snapshot.retains_imported_image(
                            asset_path,
                            image.stable_id(),
                            image.source_identity(),
                        )
                });
                self.tile_source_refresh =
                    Some(runtime.tile_set_records().iter().cloned().collect());
                self.catalog = Some(snapshot);
                self.catalog_attempt = None;
                self.catalog_failure = None;
            }
            Ok(None) => {}
            Err(error) => {
                self.catalog_attempt = None;
                self.catalog_failure = Some(error);
            }
        }
    }

    fn advance_tile_sources(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
    ) {
        let Some(catalog) = self.catalog.as_ref() else {
            return;
        };
        let Some(pending) = self.tile_source_refresh.as_mut() else {
            return;
        };
        let updates = (0..2)
            .filter_map(|_| pending.pop_front())
            .map(|record| refresh_tile_set_source(project, catalog, &record))
            .collect();
        match runtime.reconcile_tile_sets(updates) {
            Ok(demands) => {
                if pending.is_empty() {
                    self.hydration
                        .refresh_dependencies(project, catalog, &demands);
                    self.tile_source_refresh = None;
                }
            }
            Err(failure) => {
                self.runtime_revocation_failure = Some(failure);
                self.tile_source_refresh = None;
            }
        }
    }

    fn advance_import(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
    ) {
        let terminal = {
            let Some(pending) = self.pending.as_mut() else {
                return;
            };
            EditorAuthoringRuntime::poll_image_import(&mut pending.task)
        };
        let Some(terminal) = terminal else {
            return;
        };
        let pending = self
            .pending
            .take()
            .expect("a terminal image task belongs to one pending attempt");
        self.retain_import_return(&pending.task);
        let generation = pending.generation;
        let result = match terminal {
            EditorImageRuntimeTerminal::Rejected(reason) => {
                Err(EditorImageImportFailure::Runtime(reason))
            }
            EditorImageRuntimeTerminal::Imported {
                handle,
                imported,
                source,
            } => self
                .validate_completion(project, &pending, source, &imported)
                .and_then(|()| {
                    let source_digest = imported.source_digest();
                    let settings_hash = imported.artifact().key().settings_hash();
                    let version = runtime
                        .commit_image_import(imported)
                        .map_err(EditorImageImportFailure::Runtime)?;
                    let source_index_generation = project
                        .source_index_snapshot()
                        .map(|snapshot| snapshot.generation())
                        .ok_or(EditorImageImportFailure::StaleFrame)?;
                    let last_good = EditorImageLastGood::new(
                        pending.stable_id,
                        handle,
                        source.identity(),
                        EditorImageSourceProof::SourceIndex {
                            generation: source_index_generation,
                        },
                        source_digest,
                        settings_hash,
                        version,
                    );
                    if !self.last_good.insert(pending.asset_path.clone(), last_good) {
                        return Err(EditorImageImportFailure::LastGoodLimitExceeded);
                    }
                    if let Some(detail) = self
                        .latest
                        .as_ref()
                        .filter(|latest| latest.target == pending.relative_path)
                        .and_then(|latest| latest.detail.clone())
                    {
                        self.retained_details
                            .insert(Arc::clone(&pending.relative_path), detail);
                    }
                    Ok(version)
                }),
        };
        let state = match result {
            Ok(version) => EditorImageImportState::Succeeded {
                generation,
                stable_id: pending.stable_id,
                version,
            },
            Err(failure) => EditorImageImportState::Failed {
                generation,
                failure,
            },
        };
        self.set_latest_state(state);
        self.retire_metadata(pending.metadata);
    }

    fn validate_completion(
        &self,
        project: &AuthoringProjectSessionSnapshot,
        pending: &PendingImageImport,
        source: EditorImageSourceCompletion,
        imported: &nara_image::ImageImportedAsset,
    ) -> Result<(), EditorImageImportFailure> {
        let project_root = project.project_root();
        if project_root.identity() != pending.project_root_identity {
            return Err(EditorImageImportFailure::StaleFrame);
        }
        if source.identity() != pending.source_identity
            || source.digest() != imported.source_digest()
        {
            return Err(EditorImageImportFailure::SourceChanged);
        }
        pending
            .metadata
            .try_with(|host| {
                let FilePublicationBinding::Existing { identity, digest } = host.binding() else {
                    return Err(EditorImageImportFailure::MetadataPublicationUncertain);
                };
                // Keep the accepted leaf pinned while checking the current name for drift.
                let metadata = host
                    .read_source()
                    .open()
                    .map_err(|_| EditorImageImportFailure::MetadataPublicationRejected)?;
                if host.uncertain()
                    || metadata.identity() != identity
                    || metadata
                        .verify_digest(digest, metadata_byte_limit())
                        .is_err()
                {
                    return Err(EditorImageImportFailure::MetadataPublicationRejected);
                }
                Ok(())
            })
            .unwrap_or(Err(EditorImageImportFailure::MetadataPublicationRejected))?;
        if imported.image().source().stable_id() != pending.stable_id
            || imported.image().source().path() != &pending.asset_path
            || imported.artifact().key().settings_hash() != pending.settings.settings_hash()
        {
            return Err(EditorImageImportFailure::ResultMismatch);
        }
        Ok(())
    }

    fn advance_refresh(&mut self, project: &mut AuthoringProjectSessionSnapshot) {
        let Some(obligation) = self.refresh else {
            return;
        };
        if project
            .source_index_snapshot()
            .is_some_and(|index| index.generation() >= obligation.target_generation)
        {
            self.carry_source_verification_across_metadata_refresh(
                project,
                obligation.target_generation,
            );
            self.refresh = None;
            return;
        }
        if !project.source_index_has_pending_work() && project.source_index_failure().is_some() {
            self.refresh = None;
            self.reconciliation = Some(ReconciliationObligation {
                target_generation: obligation.target_generation.get(),
            });
        }
    }

    fn advance_reconciliation(&mut self, project: &AuthoringProjectSessionSnapshot) {
        let Some(obligation) = self.reconciliation else {
            return;
        };
        let Some(source_index) = project.source_index_snapshot() else {
            return;
        };
        if source_index.generation().get() < obligation.target_generation
            || !self
                .catalog
                .as_ref()
                .is_some_and(|catalog| catalog.matches_source_index(&source_index))
        {
            return;
        }
        self.reconciliation = None;
        if matches!(
            self.state(),
            EditorImageImportState::ReconcileRequired { .. }
        ) {
            self.set_latest_state(EditorImageImportState::Idle);
        }
    }

    fn carry_source_verification_across_metadata_refresh(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        target_generation: ProjectSourceIndexGeneration,
    ) {
        let Some((relative_path, asset_path, stable_id, version)) =
            self.latest.as_ref().and_then(|latest| {
                let EditorImageImportState::Succeeded {
                    stable_id, version, ..
                } = latest.state
                else {
                    return None;
                };
                latest.detail.as_ref().map(|detail| {
                    (
                        Arc::clone(&detail.relative_path),
                        detail.asset_path.clone(),
                        stable_id,
                        version,
                    )
                })
            })
        else {
            return;
        };
        let Some(source_index) = project
            .source_index_snapshot()
            .filter(|index| index.generation() >= target_generation)
        else {
            return;
        };
        let Some(last_good) = self.last_good.get_mut(&asset_path) else {
            return;
        };
        if last_good.stable_id() != stable_id
            || last_good.version() != version
            || last_good.source_proof() == EditorImageSourceProof::Unverified
            || source_index
                .entry(&relative_path)
                .and_then(crate::project_host::authoring::ProjectSourceIndexEntry::scoped_identity)
                != Some(last_good.source_identity())
        {
            return;
        }
        last_good.verify_source_index(source_index.generation());
    }

    pub(crate) fn schedule_refresh(
        &mut self,
        project: &mut AuthoringProjectSessionSnapshot,
    ) -> Option<ProjectSourceIndexGeneration> {
        self.invalidate_catalog();
        let target_generation = project.request_source_index_refresh()?;
        self.refresh = Some(RefreshObligation { target_generation });
        Some(target_generation)
    }

    /// Replaces obligations from the predecessor index after the Session installs its successor.
    /// Import results, published metadata and last-good pixels retain their original facts;
    /// predecessor index verification cannot authorize the successor's reused generations.
    pub(in crate::project_host::runtime::editor) fn adopt_schema_project_index(
        &mut self,
        project: &mut AuthoringProjectSessionSnapshot,
    ) {
        self.last_good.revoke_source_index_verification();
        self.refresh = None;
        self.reconciliation = None;
        if self.schedule_refresh(project).is_none() {
            self.catalog_failure = Some(EditorImageCatalogBuildError::SourceIndexUnavailable);
        }
    }

    fn require_reconciliation(
        &mut self,
        attempt_generation: EditorImageImportGeneration,
        target_generation: u64,
    ) {
        self.retry_publication_cleanup();
        self.reconciliation = Some(ReconciliationObligation { target_generation });
        self.set_latest_state(EditorImageImportState::ReconcileRequired {
            generation: attempt_generation,
        });
    }

    fn invalidate_catalog(&mut self) {
        self.tile_source_refresh = None;
        self.hydration.cancel_dependencies();
        self.catalog_basis = None;
        self.catalog_attempt = None;
        self.catalog = None;
        self.catalog_failure = None;
    }

    fn allocate_attempt_generation(&mut self) -> Option<EditorImageImportGeneration> {
        let generation = self.next_attempt_generation;
        self.next_attempt_generation = generation.checked_add(1)?;
        Some(EditorImageImportGeneration(generation))
    }

    fn remember_latest_detail(
        &mut self,
        relative_path: &str,
        detail_revision: u64,
        entry: &super::catalog::EditorImageCatalogEntry,
        stable_id: StableAssetId,
        settings: ImageImportSettings,
        metadata: EditorImageImportLatestMetadata,
    ) {
        let relative_path: Arc<str> = Arc::from(relative_path);
        let detail = EditorImageImportLatestDetail {
            relative_path: Arc::clone(&relative_path),
            detail_revision,
            asset_path: entry.asset_path().clone(),
            stable_id,
            settings,
            metadata,
        };
        if self.last_good.get(entry.asset_path()).is_some() {
            self.retained_details
                .insert(Arc::clone(&relative_path), detail.clone());
        }
        if let Some(latest) = self
            .latest
            .as_mut()
            .filter(|latest| latest.target == relative_path)
        {
            latest.detail = Some(detail);
        }
    }

    fn fail_request(
        &mut self,
        generation: EditorImageImportGeneration,
        failure: EditorImageImportFailure,
    ) -> EditorImageImportRequestResult {
        self.retry_publication_cleanup();
        self.set_latest_state(EditorImageImportState::Failed {
            generation,
            failure,
        });
        EditorImageImportRequestResult::Completed(self.state())
    }

    fn set_latest_state(&mut self, state: EditorImageImportState) {
        self.latest
            .as_mut()
            .expect("an allocated image import attempt owns its latest state")
            .state = state;
    }
}

fn refresh_tile_set_source(
    project: &AuthoringProjectSessionSnapshot,
    catalog: &EditorImageCatalogSnapshot,
    record: &AssetRecord,
) -> EditorTileSetSourceUpdate {
    let Some((current, metadata_identity, metadata_digest)) = catalog.asset_identity(record.path())
    else {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    };
    if &current != record {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    }
    let project_path = format!(
        "{}/{}",
        project.settings().paths.assets.as_str(),
        record.path().as_str()
    );
    let Some(index) = project.source_index_snapshot() else {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    };
    let Some(source_identity) = index
        .entry(&project_path)
        .and_then(|entry| entry.scoped_identity())
    else {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    };
    let root = project.project_root();
    let files = RelativePath::new(&project_path)
        .ok()
        .and_then(|path| root.open_file(&path).ok())
        .zip(
            RelativePath::new(format!("{project_path}.meta"))
                .ok()
                .and_then(|path| root.open_file(&path).ok()),
        );
    let Some((source_file, metadata_file)) = files else {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    };
    if source_file.identity() != source_identity
        || metadata_file.identity() != metadata_identity
        || metadata_file
            .verify_digest(metadata_digest, metadata_byte_limit())
            .is_err()
    {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    }
    let limits = TileSetSourceFileLimits::default();
    let Ok(bytes) = source_file
        .read_to_end_bounded(u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX))
    else {
        return EditorTileSetSourceUpdate::Reject(record.clone());
    };
    let digest = nara_fs::ContentDigest::of_bytes(&bytes);
    let current_source = RelativePath::new(&project_path)
        .ok()
        .and_then(|path| root.open_file(&path).ok());
    let current_metadata = RelativePath::new(format!("{project_path}.meta"))
        .ok()
        .and_then(|path| root.open_file(&path).ok());
    if current_source.is_none_or(|file| {
        file.identity() != source_identity
            || file
                .verify_digest(
                    digest,
                    u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX),
                )
                .is_err()
    }) || current_metadata.is_none_or(|file| {
        file.identity() != metadata_identity
            || file
                .verify_digest(metadata_digest, metadata_byte_limit())
                .is_err()
    }) {
        return EditorTileSetSourceUpdate::Remove(record.clone());
    }
    match TileSetSource::decode_ron_bytes(&bytes, limits) {
        Ok(source) => EditorTileSetSourceUpdate::Request(
            record.clone(),
            source,
            SourceHash::from_bytes(&bytes),
        ),
        Err(_) => EditorTileSetSourceUpdate::Reject(record.clone()),
    }
}

fn fresh_stable_id(catalog: &EditorImageCatalogSnapshot) -> Option<StableAssetId> {
    catalog.metadata_snapshot().fresh_stable_id()
}

fn map_publication_rejection(_reason: FilePublicationRejection) -> EditorImageImportFailure {
    EditorImageImportFailure::MetadataPublicationRejected
}

fn map_publication_failure(_stage: FilePublicationFailureStage) -> EditorImageImportFailure {
    EditorImageImportFailure::MetadataPublicationFailed
}
