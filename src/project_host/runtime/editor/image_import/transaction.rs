use std::{collections::BTreeMap, sync::Arc};

use nara_asset::{
    AssetMeta, AssetMetaFileLimits, AssetMetaIdentity, AssetPath, AssetRecord, AssetSourceKind,
    AssetVersion, Handle, ImportSettingsHash, StableAssetId,
};
use nara_fs::{ContentDigest, FileIdentity, RelativePath};
use nara_image::{ImageAsset, ImageImportSettings};

use crate::project_host::{
    authoring::{
        AuthoringProjectSessionSnapshot, ProjectSourceIndexGeneration, ProjectSourceIndexSnapshot,
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
use crate::project_host::runtime::editor::authoring_runtime::{
    EditorAuthoringRuntime, EditorImageRuntimeFailure, EditorImageRuntimeTask,
    EditorImageRuntimeTerminal, EditorImageSourceCompletion,
};

const MAX_STABLE_ID_ATTEMPTS: usize = 16;
const MAX_LAST_GOOD_IMAGES: usize = 4_096;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorImageLastGood {
    stable_id: StableAssetId,
    handle: Handle<ImageAsset>,
    source_identity: FileIdentity,
    source_digest: ContentDigest,
    settings_hash: ImportSettingsHash,
    version: AssetVersion,
}

impl EditorImageLastGood {
    #[must_use]
    pub(crate) const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub(crate) const fn handle(&self) -> Handle<ImageAsset> {
        self.handle
    }

    #[must_use]
    pub(crate) const fn source_identity(&self) -> FileIdentity {
        self.source_identity
    }

    #[must_use]
    pub(crate) const fn source_digest(&self) -> ContentDigest {
        self.source_digest
    }

    #[must_use]
    pub(crate) const fn settings_hash(&self) -> ImportSettingsHash {
        self.settings_hash
    }

    #[must_use]
    pub(crate) const fn version(&self) -> AssetVersion {
        self.version
    }
}

struct PendingImageImport {
    generation: EditorImageImportGeneration,
    project_root_identity: FileIdentity,
    asset_path: AssetPath,
    stable_id: StableAssetId,
    source_identity: FileIdentity,
    metadata_publication: FilePublicationHost,
    settings: ImageImportSettings,
    task: EditorImageRuntimeTask,
}

#[derive(Clone, Copy)]
struct RefreshObligation {
    target_generation: ProjectSourceIndexGeneration,
    attempt_generation: EditorImageImportGeneration,
}

#[derive(Clone, Copy)]
struct ReconciliationObligation {
    target_generation: u64,
    attempt_generation: EditorImageImportGeneration,
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
    source_identity: FileIdentity,
    settings: ImageImportSettings,
    metadata: EditorImageImportLatestMetadata,
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
    pub(super) const fn source_identity(&self) -> FileIdentity {
        self.source_identity
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
    catalog_failure: Option<EditorImageCatalogBuildError>,
    next_attempt_generation: u64,
    pending: Option<PendingImageImport>,
    latest: EditorImageImportState,
    latest_target: Option<Arc<str>>,
    latest_detail: Option<EditorImageImportLatestDetail>,
    last_good: BTreeMap<AssetPath, EditorImageLastGood>,
    refresh: Option<RefreshObligation>,
    reconciliation: Option<ReconciliationObligation>,
    #[cfg(test)]
    reject_next_valid_metadata_receipt: bool,
}

impl Default for EditorImageImportHost {
    fn default() -> Self {
        Self {
            catalog_basis: None,
            catalog_attempt: None,
            catalog: None,
            catalog_failure: None,
            next_attempt_generation: 1,
            pending: None,
            latest: EditorImageImportState::Idle,
            latest_target: None,
            latest_detail: None,
            last_good: BTreeMap::new(),
            refresh: None,
            reconciliation: None,
            #[cfg(test)]
            reject_next_valid_metadata_receipt: false,
        }
    }
}

impl EditorImageImportHost {
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
    pub(crate) const fn state(&self) -> EditorImageImportState {
        self.latest
    }

    #[must_use]
    pub(super) fn latest_target(&self) -> Option<&str> {
        self.latest_target.as_deref()
    }

    #[must_use]
    pub(super) fn latest_detail(&self) -> Option<&EditorImageImportLatestDetail> {
        self.latest_detail.as_ref()
    }

    #[must_use]
    pub(crate) fn last_good(&self, asset_path: &AssetPath) -> Option<&EditorImageLastGood> {
        self.last_good.get(asset_path)
    }

    #[must_use]
    pub(crate) fn has_pending_work(&self) -> bool {
        self.catalog_attempt.is_some() || self.pending.is_some() || self.refresh.is_some()
    }

    #[must_use]
    pub(super) fn reconciliation_generation(&self) -> Option<EditorImageImportGeneration> {
        self.reconciliation
            .map(|obligation| obligation.attempt_generation)
    }

    pub(crate) fn cancel_pending(&mut self) {
        self.pending = None;
    }

    #[cfg(test)]
    pub(crate) fn test_reject_next_valid_metadata_receipt(&mut self) {
        self.reject_next_valid_metadata_receipt = true;
    }

    pub(crate) fn advance(
        &mut self,
        project: &mut AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
    ) {
        self.advance_refresh(project);
        self.advance_catalog(project);
        self.advance_reconciliation(project);
        self.advance_import(project, runtime);
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
        if self.pending.is_some() {
            return EditorImageImportRequestResult::Rejected(EditorImageImportFailure::Busy);
        }
        if self.reconciliation.is_some() {
            return EditorImageImportRequestResult::Rejected(
                EditorImageImportFailure::ReconcileRequired,
            );
        }
        if self.refresh.is_some() {
            return EditorImageImportRequestResult::Rejected(EditorImageImportFailure::StaleFrame);
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
        if self.last_good.len() >= MAX_LAST_GOOD_IMAGES
            && !self.last_good.contains_key(entry.asset_path())
        {
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
        self.latest_target = Some(Arc::from(project_relative_path));
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
        let (stable_id, previous_settings, mut publication) = match entry.metadata() {
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
                let publication = match FilePublicationHost::bind_existing(
                    &project_root,
                    entry.metadata_project_relative_path(),
                    receipt.identity(),
                    receipt.digest(),
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
                (receipt.stable_id(), Some(receipt.settings()), publication)
            }
            EditorImageMetadataStatus::Invalid(_)
            | EditorImageMetadataStatus::IdentityConflict { .. } => {
                return self.fail_request(generation, EditorImageImportFailure::MetadataInvalid);
            }
        };

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
        let mut detail_revision = source_index.generation().get();
        if metadata_changed {
            match publication.publish(encoded.as_bytes()) {
                FilePublicationOutcome::Published(candidate) => {
                    #[cfg(test)]
                    if std::mem::take(&mut self.reject_next_valid_metadata_receipt) {
                        let _ = candidate.into_parts();
                        publication.mark_uncertain();
                        let target_generation = self
                            .schedule_refresh(project, generation)
                            .map(ProjectSourceIndexGeneration::get)
                            .unwrap_or_else(|| source_index.generation().get().saturating_add(1));
                        self.remember_latest_detail(
                            project_relative_path,
                            target_generation,
                            &entry,
                            stable_id,
                            source_identity,
                            settings,
                            EditorImageImportLatestMetadata::Uncertain,
                        );
                        self.require_reconciliation(generation, target_generation);
                        return EditorImageImportRequestResult::Completed(self.latest);
                    }
                    let (_, binding) = candidate.into_parts();
                    publication.commit_published(binding);
                    let target_generation = self.schedule_refresh(project, generation);
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
                            source_identity,
                            settings,
                            EditorImageImportLatestMetadata::Ready,
                        );
                        self.require_reconciliation(generation, detail_revision);
                        return EditorImageImportRequestResult::Completed(self.latest);
                    };
                }
                FilePublicationOutcome::Rejected(reason) => {
                    return self.fail_request(generation, map_publication_rejection(reason));
                }
                FilePublicationOutcome::Failed(stage) => {
                    return self.fail_request(generation, map_publication_failure(stage));
                }
                FilePublicationOutcome::Uncertain(_) => {
                    let target_generation = self
                        .schedule_refresh(project, generation)
                        .map(ProjectSourceIndexGeneration::get)
                        .unwrap_or_else(|| source_index.generation().get().saturating_add(1));
                    self.remember_latest_detail(
                        project_relative_path,
                        target_generation,
                        &entry,
                        stable_id,
                        source_identity,
                        settings,
                        EditorImageImportLatestMetadata::Uncertain,
                    );
                    self.require_reconciliation(generation, target_generation);
                    return EditorImageImportRequestResult::Completed(self.latest);
                }
            }
        }
        if !matches!(
            publication.binding(),
            FilePublicationBinding::Existing { .. }
        ) {
            return self.fail_request(
                generation,
                EditorImageImportFailure::MetadataPublicationUncertain,
            );
        }
        self.remember_latest_detail(
            project_relative_path,
            detail_revision,
            &entry,
            stable_id,
            source_identity,
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
            asset_path: entry.asset_path().clone(),
            stable_id,
            source_identity,
            metadata_publication: publication,
            settings,
            task,
        });
        self.latest = EditorImageImportState::Pending { generation };
        EditorImageImportRequestResult::Started(generation)
    }

    fn advance_catalog(&mut self, project: &AuthoringProjectSessionSnapshot) {
        if self.refresh.is_some() || project.source_index_failure().is_some() {
            self.catalog_basis = None;
            self.catalog_attempt = None;
            self.catalog = None;
            self.catalog_failure = None;
            return;
        }
        let Some(source_index) = project.source_index_snapshot() else {
            self.catalog_basis = None;
            self.catalog_attempt = None;
            self.catalog = None;
            self.catalog_failure = None;
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
                self.catalog = Some(Arc::new(snapshot));
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
                    let version = runtime.commit_image_import(imported).map_err(|_| {
                        EditorImageImportFailure::Runtime(EditorImageRuntimeFailure::Publication)
                    })?;
                    let last_good = EditorImageLastGood {
                        stable_id: pending.stable_id,
                        handle,
                        source_identity: source.identity(),
                        source_digest,
                        settings_hash,
                        version,
                    };
                    self.last_good.insert(pending.asset_path.clone(), last_good);
                    Ok(version)
                }),
        };
        self.latest = match result {
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
        let FilePublicationBinding::Existing {
            identity: metadata_identity,
            digest: metadata_digest,
        } = pending.metadata_publication.binding()
        else {
            return Err(EditorImageImportFailure::MetadataPublicationUncertain);
        };
        let metadata = pending
            .metadata_publication
            .open_target()
            .map_err(|_| EditorImageImportFailure::MetadataPublicationRejected)?;
        if metadata.identity() != metadata_identity
            || metadata
                .verify_digest(metadata_digest, metadata_byte_limit())
                .is_err()
        {
            return Err(EditorImageImportFailure::MetadataPublicationRejected);
        }
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
            self.refresh = None;
            return;
        }
        if !project.source_index_has_pending_work() && project.source_index_failure().is_some() {
            self.refresh = None;
            self.reconciliation = Some(ReconciliationObligation {
                target_generation: obligation.target_generation.get(),
                attempt_generation: obligation.attempt_generation,
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
            self.latest,
            EditorImageImportState::ReconcileRequired { .. }
        ) {
            self.latest = EditorImageImportState::Idle;
        }
    }

    fn schedule_refresh(
        &mut self,
        project: &mut AuthoringProjectSessionSnapshot,
        attempt_generation: EditorImageImportGeneration,
    ) -> Option<ProjectSourceIndexGeneration> {
        self.invalidate_catalog();
        let target_generation = project.request_source_index_refresh()?;
        self.refresh = Some(RefreshObligation {
            target_generation,
            attempt_generation,
        });
        Some(target_generation)
    }

    fn require_reconciliation(
        &mut self,
        attempt_generation: EditorImageImportGeneration,
        target_generation: u64,
    ) {
        self.reconciliation = Some(ReconciliationObligation {
            target_generation,
            attempt_generation,
        });
        self.latest = EditorImageImportState::ReconcileRequired {
            generation: attempt_generation,
        };
    }

    fn invalidate_catalog(&mut self) {
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
        source_identity: FileIdentity,
        settings: ImageImportSettings,
        metadata: EditorImageImportLatestMetadata,
    ) {
        self.latest_detail = Some(EditorImageImportLatestDetail {
            relative_path: Arc::from(relative_path),
            detail_revision,
            asset_path: entry.asset_path().clone(),
            stable_id,
            source_identity,
            settings,
            metadata,
        });
    }

    fn fail_request(
        &mut self,
        generation: EditorImageImportGeneration,
        failure: EditorImageImportFailure,
    ) -> EditorImageImportRequestResult {
        self.latest = EditorImageImportState::Failed {
            generation,
            failure,
        };
        EditorImageImportRequestResult::Completed(self.latest)
    }
}

fn fresh_stable_id(catalog: &EditorImageCatalogSnapshot) -> Option<StableAssetId> {
    (0..MAX_STABLE_ID_ATTEMPTS)
        .filter_map(|_| StableAssetId::try_random().ok())
        .find(|stable_id| !catalog.contains_stable_id(*stable_id))
}

fn metadata_byte_limit() -> u64 {
    u64::try_from(AssetMetaFileLimits::default().encoded_bytes().get()).unwrap_or(u64::MAX)
}

fn map_publication_rejection(_reason: FilePublicationRejection) -> EditorImageImportFailure {
    EditorImageImportFailure::MetadataPublicationRejected
}

fn map_publication_failure(_stage: FilePublicationFailureStage) -> EditorImageImportFailure {
    EditorImageImportFailure::MetadataPublicationFailed
}
