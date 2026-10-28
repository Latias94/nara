use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use nara_asset::{AssetPath, AssetRecord, AssetRef, AssetSourceKind, Handle, StableAssetId};
use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, RelativePath};
use nara_image::{ImageAsset, ImageImportSettings, ImageImportedAsset};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskRejectReason, TaskReturnState, TaskSpawnRequest,
    TaskTerminal,
};

use crate::project_host::authoring::{
    AuthoringAssetSource, AuthoringDocumentCapture, AuthoringProjectAttemptGeneration,
    AuthoringProjectSessionSnapshot,
};
use crate::project_host::runtime::editor::authoring_runtime::{
    EditorAuthoringRuntime, EditorImageRuntimeFailure, EditorImageRuntimeTask,
    EditorImageRuntimeTerminal, EditorImageSourceCompletion,
};

use super::catalog::{EditorImageCatalogSnapshot, EditorImageMetadataStatus};
use super::store::{
    EditorImageLastGood, EditorImageLastGoodStore, EditorImageSourceProof, MAX_LAST_GOOD_IMAGES,
};

#[cfg(all(test, any(windows, target_os = "linux")))]
#[path = "capture_hydration.rs"]
mod capture_hydration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorImageHydrationFailureKind {
    ClosureInvalid,
    CapacityExceeded,
    SourceUnavailable,
    SourceChanged,
    MetadataUnavailable,
    MetadataChanged,
    Runtime(EditorImageRuntimeFailure),
    ResultMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorImageHydrationFailure {
    asset_path: AssetPath,
    kind: EditorImageHydrationFailureKind,
}

impl EditorImageHydrationFailure {
    #[must_use]
    pub(crate) const fn asset_path(&self) -> &AssetPath {
        &self.asset_path
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> EditorImageHydrationFailureKind {
        self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditorImageHydrationSource {
    attempt_generation: AuthoringProjectAttemptGeneration,
    project_root_identity: FileIdentity,
    record: AssetRecord,
    source_path: RelativePath,
    source_identity: FileIdentity,
    source_digest: Option<ContentDigest>,
    metadata_path: RelativePath,
    metadata_identity: FileIdentity,
    metadata_digest: ContentDigest,
    settings: ImageImportSettings,
}

impl EditorImageHydrationSource {
    fn captured(
        project: &AuthoringProjectSessionSnapshot,
        source: &AuthoringAssetSource,
    ) -> Result<Self, EditorImageHydrationFailure> {
        let invalid = || {
            failure(
                source.record().path().clone(),
                EditorImageHydrationFailureKind::ClosureInvalid,
            )
        };
        let metadata = source
            .metadata()
            .admit::<ImageImportSettings>()
            .map_err(|_| invalid())?;
        let record = source.record();
        if record.source_kind() != &AssetSourceKind::Image
            || metadata.identity().stable_id() != record.stable_id()
            || metadata.identity().path() != record.path()
            || metadata.identity().source_kind() != record.source_kind()
        {
            return Err(invalid());
        }
        Ok(Self {
            attempt_generation: project.attempt_generation(),
            project_root_identity: project.project_root().identity(),
            record: record.clone(),
            source_path: RelativePath::new(source.source().project_path().as_str())
                .map_err(|_| invalid())?,
            source_identity: source.source().identity(),
            source_digest: Some(source.source().digest()),
            metadata_path: RelativePath::new(source.metadata_source().project_path().as_str())
                .map_err(|_| invalid())?,
            metadata_identity: source.metadata_source().identity(),
            metadata_digest: source.metadata_source().digest(),
            settings: *metadata.settings(),
        })
    }

    #[must_use]
    fn asset_path(&self) -> &AssetPath {
        self.record.path()
    }

    #[must_use]
    fn stable_id(&self) -> StableAssetId {
        self.record.stable_id()
    }

    fn matches_last_good(&self, store: &EditorImageLastGoodStore) -> bool {
        store.get(self.asset_path()).is_some_and(|last_good| {
            last_good.stable_id() == self.stable_id()
                && last_good.source_identity() == self.source_identity
                && Some(last_good.source_digest()) == self.source_digest
                && last_good.settings_hash() == self.settings.settings_hash()
        })
    }

    fn validate_current_files(
        &self,
        project_root: &DirectoryCapability,
        source_digest: ContentDigest,
    ) -> Result<(), EditorImageHydrationFailureKind> {
        let source = project_root
            .open_file_exact(&self.source_path)
            .map_err(|_| EditorImageHydrationFailureKind::SourceUnavailable)?;
        if source.identity() != self.source_identity
            || source
                .verify_digest(source_digest, source_digest.length())
                .is_err()
        {
            return Err(EditorImageHydrationFailureKind::SourceChanged);
        }
        let metadata = project_root
            .open_file_exact(&self.metadata_path)
            .map_err(|_| EditorImageHydrationFailureKind::MetadataUnavailable)?;
        if metadata.identity() != self.metadata_identity
            || metadata
                .verify_digest(self.metadata_digest, super::metadata_byte_limit())
                .is_err()
        {
            return Err(EditorImageHydrationFailureKind::MetadataChanged);
        }
        Ok(())
    }

    fn validate_observation(
        &self,
        root: &DirectoryCapability,
        digest: ContentDigest,
        catalog: Option<&EditorImageCatalogSnapshot>,
    ) -> Result<(), EditorImageHydrationFailureKind> {
        self.validate_current_files(root, digest)?;
        if let Some(catalog) = catalog {
            let conflict = catalog
                .has_current_identity_conflict(root, self.asset_path(), self.stable_id())
                .map_err(|_| EditorImageHydrationFailureKind::MetadataUnavailable)?;
            if conflict {
                return Err(EditorImageHydrationFailureKind::MetadataChanged);
            }
        }
        Ok(())
    }
}

struct PendingImageHydration {
    source: EditorImageHydrationSource,
    work: ImageHydrationWork,
    cancelled: bool,
}

enum ImageHydrationWork {
    Import(EditorImageRuntimeTask),
    Validate {
        task: TaskHandle<ValidatedImageHydration>,
        observation: Option<Arc<EditorImageCatalogSnapshot>>,
    },
    Recheck {
        task: TaskHandle<Result<(), EditorImageHydrationFailureKind>>,
        image: EditorImageLastGood,
        observation: Option<Arc<EditorImageCatalogSnapshot>>,
    },
}

struct DecodedImageHydration {
    handle: Handle<ImageAsset>,
    imported: ImageImportedAsset,
}

struct ValidatedImageHydration {
    decoded: DecodedImageHydration,
    result: Result<(), EditorImageHydrationFailureKind>,
}

impl PendingImageHydration {
    fn cancel(&mut self) {
        self.cancelled = true;
        match &self.work {
            ImageHydrationWork::Import(task) => {
                task.cancel();
            }
            ImageHydrationWork::Validate { task, .. } => {
                task.cancel();
            }
            ImageHydrationWork::Recheck { task, .. } => {
                task.cancel();
            }
        }
    }

    fn return_state(&self) -> TaskReturnState {
        match &self.work {
            ImageHydrationWork::Import(task) => task.return_observer().state(),
            ImageHydrationWork::Validate { task, .. } => task.return_observer().state(),
            ImageHydrationWork::Recheck { task, .. } => task.return_observer().state(),
        }
    }
}

const HYDRATION_VALIDATION_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x696d_675f_7072_6f6f);

#[derive(Default)]
pub(super) struct EditorImageHydrationHost {
    initialized_attempt: Option<AuthoringProjectAttemptGeneration>,
    next_admission_tick: Option<u64>,
    queued: VecDeque<EditorImageHydrationSource>,
    rechecks: VecDeque<(EditorImageHydrationSource, EditorImageLastGood)>,
    catalog_observation: Option<Arc<EditorImageCatalogSnapshot>>,
    pending: Option<PendingImageHydration>,
    failure: Option<EditorImageHydrationFailure>,
    revocation_failure: Option<EditorImageRuntimeFailure>,
    #[cfg(test)]
    next_validation_gate: Option<Box<dyn FnOnce() + Send>>,
}

impl EditorImageHydrationHost {
    pub(super) fn initialize(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        store: &mut EditorImageLastGoodStore,
    ) {
        let attempt_generation = project.attempt_generation();
        if self.initialized_attempt == Some(attempt_generation) {
            return;
        }
        self.cancel_pending();
        if self.initialized_attempt.is_some() {
            self.catalog_observation = None;
        }
        self.initialized_attempt = Some(attempt_generation);
        self.next_admission_tick = Some(attempt_generation.get());
        self.failure = None;
        self.revocation_failure = None;
        self.enqueue_authoring_sources(project, project.image_asset_sources(), store);
    }

    pub(super) fn enqueue_authoring_capture(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        capture: &AuthoringDocumentCapture,
        store: &mut EditorImageLastGoodStore,
    ) {
        self.initialize(project, store);
        self.enqueue_authoring_sources(
            project,
            capture
                .assets()
                .iter()
                .filter(|source| source.record().source_kind() == &AssetSourceKind::Image),
            store,
        );
    }

    fn enqueue_authoring_sources<'a>(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        sources: impl Iterator<Item = &'a AuthoringAssetSource>,
        store: &mut EditorImageLastGoodStore,
    ) {
        for source in sources {
            match EditorImageHydrationSource::captured(project, source) {
                Ok(source) => self.enqueue_source(source, store),
                Err(failure) => self.remember_failure(failure),
            }
        }
    }

    fn enqueue_source(
        &mut self,
        source: EditorImageHydrationSource,
        store: &mut EditorImageLastGoodStore,
    ) {
        // Captures feed a shared image cache. Only completion's current file proof may
        // publish pixels; a newer request replaces the older receipt for this path.
        let cached = source.matches_last_good(store);
        if cached {
            let image = store
                .get_mut(source.asset_path())
                .expect("matching cached pixels retain their image receipt");
            image.refresh_capture_source(source.clone());
            let image = image.clone();
            self.queued
                .retain(|queued| queued.asset_path() != source.asset_path());
            if let Some(pending) = &mut self.pending {
                if pending.source.asset_path() == source.asset_path() {
                    if !pending.cancelled
                        && pending.source == source
                        && matches!(&pending.work,
                            ImageHydrationWork::Recheck { image: expected, .. } if expected == &image)
                    {
                        return;
                    }
                    pending.cancel();
                }
            }
            self.rechecks
                .retain(|(check, _)| check.asset_path() != source.asset_path());
            if self.catalog_observation.as_ref().is_some_and(|catalog| {
                !catalog.retains_imported_image(
                    source.asset_path(),
                    source.stable_id(),
                    source.source_identity,
                )
            }) {
                self.queue_recheck(source, image);
            }
            return;
        }
        if self.pending.as_ref().is_some_and(|pending| {
            pending.source.asset_path() == source.asset_path()
                && !matches!(pending.work, ImageHydrationWork::Recheck { .. })
        }) {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| !pending.cancelled && pending.source == source)
            {
                return;
            }
            self.pending
                .as_mut()
                .expect("the matching hydration remains owned")
                .cancel();
        }
        if let Some(position) = self
            .queued
            .iter()
            .position(|queued| queued.asset_path() == source.asset_path())
        {
            if self.queued[position] != source {
                self.queued[position] = source;
            }
            return;
        }
        if self.queued.len()
            + usize::from(
                self.pending
                    .as_ref()
                    .is_some_and(|pending| !pending.cancelled),
            )
            >= MAX_LAST_GOOD_IMAGES
        {
            self.remember_failure(failure(
                source.asset_path().clone(),
                EditorImageHydrationFailureKind::CapacityExceeded,
            ));
            return;
        }
        self.queued.push_back(source);
    }

    pub(super) fn refresh_dependencies(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        catalog: &EditorImageCatalogSnapshot,
        demands: &[AssetRef],
    ) {
        let entries = demands
            .iter()
            .filter_map(|reference| catalog.image_entry(reference))
            .collect::<Vec<_>>();
        self.queued.retain(|source| {
            source.source_digest.is_some()
                || entries.iter().any(|entry| {
                    entry.asset_path() == source.asset_path()
                        && matches!(entry.metadata(), EditorImageMetadataStatus::Ready(metadata)
                    if metadata.stable_id() == source.stable_id()
                        && metadata.digest() == source.metadata_digest)
                })
        });
        if self.pending.as_ref().is_some_and(|pending| {
            pending.source.source_digest.is_none()
                && !entries.iter().any(|entry| {
                    entry.asset_path() == pending.source.asset_path()
                        && matches!(entry.metadata(), EditorImageMetadataStatus::Ready(metadata)
                        if metadata.stable_id() == pending.source.stable_id()
                            && metadata.digest() == pending.source.metadata_digest)
                })
        }) {
            self.pending
                .as_mut()
                .expect("the obsolete dependency remains owned")
                .cancel();
        }
        for entry in entries {
            if self
                .queued
                .iter()
                .any(|source| source.asset_path() == entry.asset_path())
                || self.pending.as_ref().is_some_and(|pending| {
                    !pending.cancelled && pending.source.asset_path() == entry.asset_path()
                })
            {
                continue;
            }
            if self.queued.len()
                + usize::from(
                    self.pending
                        .as_ref()
                        .is_some_and(|pending| !pending.cancelled),
                )
                >= MAX_LAST_GOOD_IMAGES
            {
                self.remember_failure(failure(
                    entry.asset_path().clone(),
                    EditorImageHydrationFailureKind::CapacityExceeded,
                ));
                break;
            }
            let EditorImageMetadataStatus::Ready(metadata) = entry.metadata() else {
                continue;
            };
            let Some(source_identity) = entry.source_identity() else {
                continue;
            };
            let (Ok(source_path), Ok(metadata_path)) = (
                RelativePath::new(entry.project_relative_path()),
                RelativePath::new(entry.metadata_project_relative_path()),
            ) else {
                continue;
            };
            self.queued.push_back(EditorImageHydrationSource {
                attempt_generation: project.attempt_generation(),
                project_root_identity: project.project_root().identity(),
                record: AssetRecord::new(
                    metadata.stable_id(),
                    entry.asset_path().clone(),
                    AssetSourceKind::Image,
                ),
                source_path,
                source_identity,
                source_digest: None,
                metadata_path,
                metadata_identity: metadata.identity(),
                metadata_digest: metadata.digest(),
                settings: metadata.settings(),
            });
        }
    }

    #[must_use]
    pub(super) fn has_pending_work(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| !pending.cancelled)
            || !self.queued.is_empty()
            || !self.rechecks.is_empty()
    }

    /// Cancelled hydration still owns its task until the executor has physically returned it.
    #[must_use]
    pub(super) fn schema_adoption_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Cancels and observes admitted work without starting queued imports or validation.
    /// Retain its input so ordinary driving can resume if adoption is refused.
    pub(super) fn observe_schema_adoption_drain(&mut self) {
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        if !pending.cancelled {
            match &pending.work {
                ImageHydrationWork::Recheck { image, .. } => {
                    self.queue_recheck(pending.source.clone(), image.clone());
                }
                ImageHydrationWork::Import(_) | ImageHydrationWork::Validate { .. } => {
                    if !self
                        .queued
                        .iter()
                        .any(|source| source.asset_path() == pending.source.asset_path())
                    {
                        self.queued.push_front(pending.source.clone());
                    }
                }
            }
            pending.cancel();
        }
        if pending.return_state() == TaskReturnState::Pending {
            self.pending = Some(pending);
        }
    }

    pub(super) fn cancel_pending(&mut self) {
        if let Some(pending) = &mut self.pending {
            pending.cancel();
        }
        self.queued.clear();
        self.rechecks.clear();
    }

    pub(super) fn cancel_dependencies(&mut self) {
        self.queued.retain(|source| source.source_digest.is_some());
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.source.source_digest.is_none())
        {
            self.pending
                .as_mut()
                .expect("the dependency hydration remains owned")
                .cancel();
        }
    }

    pub(super) fn surrender_source(&mut self, asset_path: &AssetPath) {
        self.queued
            .retain(|source| source.asset_path() != asset_path);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.source.asset_path() == asset_path)
        {
            self.pending
                .as_mut()
                .expect("the surrendered hydration remains owned")
                .cancel();
        }
        self.rechecks
            .retain(|(source, _)| source.asset_path() != asset_path);
    }

    pub(super) fn retain_catalog_eligible(
        &mut self,
        mut retain: impl FnMut(&AssetPath, StableAssetId, FileIdentity) -> bool,
    ) {
        self.queued.retain(|source| {
            source.source_digest.is_some()
                || retain(
                    source.asset_path(),
                    source.stable_id(),
                    source.source_identity,
                )
        });
        if self.pending.as_ref().is_some_and(|pending| {
            pending.source.source_digest.is_none()
                && !retain(
                    pending.source.asset_path(),
                    pending.source.stable_id(),
                    pending.source.source_identity,
                )
        }) {
            self.pending
                .as_mut()
                .expect("the ineligible hydration remains owned")
                .cancel();
        }
    }

    /// Each installed catalogue is a distinct observation, including when its index is older
    /// than a capture. A worker may only settle the observation it actually checked.
    pub(super) fn observe_catalog(
        &mut self,
        catalog: Arc<EditorImageCatalogSnapshot>,
        store: &EditorImageLastGoodStore,
    ) -> BTreeSet<AssetPath> {
        self.retain_catalog_eligible(|path, id, identity| {
            catalog.retains_imported_image(path, id, identity)
        });
        self.catalog_observation = Some(catalog);
        let mut deferred = BTreeSet::new();
        for (path, image) in store.iter() {
            if self.catalog_observation.as_ref().is_some_and(|catalog| {
                catalog.retains_imported_image(path, image.stable_id(), image.source_identity())
            }) {
                continue;
            }
            let Some(source) = image.capture_source() else {
                continue;
            };
            deferred.insert(path.clone());
            let pending = self.pending.as_ref().is_some_and(|pending| {
                !pending.cancelled
                    && matches!(&pending.work,
                    ImageHydrationWork::Recheck { image: expected, observation, .. }
                        if expected == image && self.observation_matches(observation))
            });
            if !pending {
                self.queue_recheck(source.clone(), image.clone());
            }
        }
        deferred
    }

    fn queue_recheck(&mut self, source: EditorImageHydrationSource, image: EditorImageLastGood) {
        if let Some(queued) = self
            .rechecks
            .iter_mut()
            .find(|(expected, _)| expected.asset_path() == source.asset_path())
        {
            *queued = (source, image);
        } else {
            self.rechecks.push_back((source, image));
        }
    }

    fn observation_matches(&self, observation: &Option<Arc<EditorImageCatalogSnapshot>>) -> bool {
        match (&self.catalog_observation, observation) {
            (Some(current), Some(expected)) => Arc::ptr_eq(current, expected),
            (None, None) => true,
            _ => false,
        }
    }

    pub(super) fn take_revocation_failure(&mut self) -> Option<EditorImageRuntimeFailure> {
        self.revocation_failure.take()
    }

    pub(super) fn take_failure(&mut self) -> Option<EditorImageHydrationFailure> {
        self.failure.take()
    }

    pub(super) fn advance(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
        store: &mut EditorImageLastGoodStore,
    ) {
        if self.advance_pending(project, runtime, store) {
            return;
        }
        while let Some((source, image)) = self.rechecks.pop_front() {
            if store.get(source.asset_path()) != Some(&image) {
                continue;
            }
            let Some(tick) = self.allocate_admission_tick() else {
                return;
            };
            let root = project.project_root();
            let expected = source.clone();
            let digest = image.source_digest();
            let observation = self.catalog_observation.clone();
            let worker_observation = observation.clone();
            #[cfg(test)]
            let gate = self.next_validation_gate.take();
            let task = runtime
                .persistence_tasks()
                .spawn(
                    TaskPoolKind::Io,
                    TaskSpawnRequest::new(tick, HYDRATION_VALIDATION_DOMAIN),
                    move |_| {
                        #[cfg(test)]
                        if let Some(gate) = gate {
                            gate();
                        }
                        expected.validate_observation(&root, digest, worker_observation.as_deref())
                    },
                )
                .into_handle();
            match task {
                Ok(task) => {
                    self.pending = Some(PendingImageHydration {
                        source,
                        work: ImageHydrationWork::Recheck {
                            task,
                            image,
                            observation,
                        },
                        cancelled: false,
                    });
                }
                Err(_) => {
                    self.rechecks.push_front((source, image));
                }
            }
            return;
        }
        let Some(source) = self.queued.pop_front() else {
            return;
        };
        let asset_path = source.asset_path().clone();
        if !store.can_insert(&asset_path) {
            self.remember_failure(failure(
                asset_path,
                EditorImageHydrationFailureKind::CapacityExceeded,
            ));
            return;
        }
        if !self.current_authoring_attempt_matches(project, &source) {
            self.remember_failure(failure(
                asset_path,
                EditorImageHydrationFailureKind::SourceChanged,
            ));
            return;
        }
        let project_root = project.project_root();
        let source_file = match project_root.open_file_exact(&source.source_path) {
            Ok(file) if file.identity() == source.source_identity => file,
            Ok(_) => {
                self.remember_failure(failure(
                    asset_path,
                    EditorImageHydrationFailureKind::SourceChanged,
                ));
                return;
            }
            Err(_) => {
                self.remember_failure(failure(
                    asset_path,
                    EditorImageHydrationFailureKind::SourceUnavailable,
                ));
                return;
            }
        };
        let Some(admission_tick) = self.allocate_admission_tick() else {
            self.remember_failure(failure(
                asset_path,
                EditorImageHydrationFailureKind::Runtime(EditorImageRuntimeFailure::TaskAdmission),
            ));
            return;
        };
        let task = match runtime.spawn_image_import(
            admission_tick,
            source.record.clone(),
            source_file,
            source.source_identity,
            project_root.share(),
            source.source_path.clone(),
            source.settings,
        ) {
            Ok(task) => task,
            Err(reason) => {
                self.remember_failure(failure(
                    asset_path,
                    EditorImageHydrationFailureKind::Runtime(reason),
                ));
                return;
            }
        };
        self.pending = Some(PendingImageHydration {
            source,
            work: ImageHydrationWork::Import(task),
            cancelled: false,
        });
    }

    fn advance_pending(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
        store: &mut EditorImageLastGoodStore,
    ) -> bool {
        let Some(mut pending) = self.pending.take() else {
            return false;
        };
        if pending.return_state() == TaskReturnState::Pending {
            self.pending = Some(pending);
            return true;
        }
        if pending.cancelled {
            return false;
        }
        let asset_path = pending.source.asset_path().clone();
        let result = match pending.work {
            ImageHydrationWork::Import(ref mut task) => {
                match EditorAuthoringRuntime::poll_image_import(task) {
                    Some(EditorImageRuntimeTerminal::Imported {
                        handle,
                        imported,
                        source,
                    }) => {
                        if let Err(kind) =
                            self.validate_completion(project, &pending.source, source, &imported)
                        {
                            self.remember_failure(failure(asset_path, kind));
                            return true;
                        }
                        self.start_validation(
                            project,
                            runtime,
                            pending.source,
                            DecodedImageHydration { handle, imported },
                        );
                        return true;
                    }
                    Some(EditorImageRuntimeTerminal::Rejected(reason)) => {
                        Err(EditorImageHydrationFailureKind::Runtime(reason))
                    }
                    None => Err(EditorImageHydrationFailureKind::Runtime(
                        EditorImageRuntimeFailure::TaskFailed,
                    )),
                }
            }
            ImageHydrationWork::Validate {
                mut task,
                observation,
            } => match task.try_take() {
                Some(TaskTerminal::Completed(validated)) => {
                    if !self.observation_matches(&observation) {
                        self.start_validation(project, runtime, pending.source, validated.decoded);
                        return true;
                    }
                    let DecodedImageHydration { handle, imported } = validated.decoded;
                    (|| {
                        validated.result?;
                        if !self.current_authoring_attempt_matches(project, &pending.source) {
                            return Err(EditorImageHydrationFailureKind::SourceChanged);
                        }
                        if !store.can_insert(&asset_path) {
                            return Err(EditorImageHydrationFailureKind::CapacityExceeded);
                        }
                        let proof = if pending.source.source_digest.is_some() {
                            EditorImageSourceProof::AuthoringClosure {
                                attempt_generation: pending.source.attempt_generation,
                            }
                        } else {
                            EditorImageSourceProof::SourceIndex {
                                generation: project
                                    .source_index_snapshot()
                                    .ok_or(EditorImageHydrationFailureKind::SourceChanged)?
                                    .generation(),
                            }
                        };
                        let source_digest = imported.source_digest();
                        let settings_hash = imported.artifact().key().settings_hash();
                        let version = runtime.commit_image_import(imported).map_err(|_| {
                            EditorImageHydrationFailureKind::Runtime(
                                EditorImageRuntimeFailure::Publication,
                            )
                        })?;
                        let last_good = EditorImageLastGood::new(
                            pending.source.stable_id(),
                            handle,
                            pending.source.source_identity,
                            proof,
                            source_digest,
                            settings_hash,
                            version,
                        );
                        let last_good = if pending.source.source_digest.is_some() {
                            last_good.with_capture_source(pending.source)
                        } else {
                            last_good
                        };
                        if store.insert(asset_path.clone(), last_good) {
                            Ok(())
                        } else {
                            Err(EditorImageHydrationFailureKind::CapacityExceeded)
                        }
                    })()
                }
                _ => Err(EditorImageHydrationFailureKind::Runtime(
                    EditorImageRuntimeFailure::TaskFailed,
                )),
            },
            ImageHydrationWork::Recheck {
                mut task,
                image,
                observation,
            } => {
                if store.get(&asset_path) != Some(&image) {
                    return true;
                }
                if !self.observation_matches(&observation) {
                    self.queue_recheck(pending.source, image);
                    return true;
                }
                let result = match task.try_take() {
                    Some(TaskTerminal::Completed(result)) => result,
                    _ => Err(EditorImageHydrationFailureKind::Runtime(
                        EditorImageRuntimeFailure::TaskFailed,
                    )),
                };
                let result = result.and_then(|()| {
                    if self.current_authoring_attempt_matches(project, &pending.source) {
                        Ok(())
                    } else {
                        Err(EditorImageHydrationFailureKind::SourceChanged)
                    }
                });
                if result.is_err() {
                    // Retirement of choices is terminal even when runtime retirement fails.
                    // The transaction owner latches that exact failure and blocks further writes.
                    store.remove_if_matches(&asset_path, &image);
                    if let Err(reason) = runtime.revoke_images(vec![image.handle()]) {
                        self.revocation_failure = Some(reason);
                        self.cancel_pending();
                        return true;
                    }
                }
                result
            }
        };
        if let Err(kind) = result {
            self.remember_failure(failure(asset_path, kind));
        }
        true
    }

    fn start_validation(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &EditorAuthoringRuntime,
        source: EditorImageHydrationSource,
        decoded: DecodedImageHydration,
    ) {
        let Some(tick) = self.allocate_admission_tick() else {
            self.remember_failure(failure(
                source.asset_path().clone(),
                EditorImageHydrationFailureKind::Runtime(EditorImageRuntimeFailure::TaskAdmission),
            ));
            return;
        };
        let expected = source.clone();
        let root = project.project_root();
        let observation = self.catalog_observation.clone();
        let worker_observation = observation.clone();
        #[cfg(test)]
        let gate = self.next_validation_gate.take();
        let task = runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(tick, HYDRATION_VALIDATION_DOMAIN),
                move |_| {
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate();
                    }
                    let result = expected.validate_observation(
                        &root,
                        decoded.imported.source_digest(),
                        worker_observation.as_deref(),
                    );
                    ValidatedImageHydration { decoded, result }
                },
            )
            .into_handle();
        match task {
            Ok(task) => {
                self.pending = Some(PendingImageHydration {
                    source,
                    work: ImageHydrationWork::Validate { task, observation },
                    cancelled: false,
                });
            }
            Err(rejection) if matches!(rejection.reason, TaskRejectReason::QueueFull { .. }) => {
                self.queued.push_front(source);
            }
            Err(_) => self.remember_failure(failure(
                source.asset_path().clone(),
                EditorImageHydrationFailureKind::Runtime(EditorImageRuntimeFailure::TaskAdmission),
            )),
        }
    }

    fn validate_completion(
        &self,
        project: &AuthoringProjectSessionSnapshot,
        expected: &EditorImageHydrationSource,
        completed_source: EditorImageSourceCompletion,
        imported: &ImageImportedAsset,
    ) -> Result<(), EditorImageHydrationFailureKind> {
        if !self.current_authoring_attempt_matches(project, expected) {
            return Err(EditorImageHydrationFailureKind::SourceChanged);
        }
        if completed_source.identity() != expected.source_identity
            || completed_source.digest() != imported.source_digest()
            || expected
                .source_digest
                .is_some_and(|digest| completed_source.digest() != digest)
        {
            return Err(EditorImageHydrationFailureKind::SourceChanged);
        }
        if imported.image().source().stable_id() != expected.stable_id()
            || imported.image().source().path() != expected.asset_path()
            || imported.image().source().settings() != expected.settings
            || imported.artifact().key().settings_hash() != expected.settings.settings_hash()
        {
            return Err(EditorImageHydrationFailureKind::ResultMismatch);
        }
        Ok(())
    }

    fn current_authoring_attempt_matches(
        &self,
        project: &AuthoringProjectSessionSnapshot,
        expected: &EditorImageHydrationSource,
    ) -> bool {
        project.attempt_generation() == expected.attempt_generation
            && project.project_root().identity() == expected.project_root_identity
    }

    fn allocate_admission_tick(&mut self) -> Option<u64> {
        let admission_tick = self.next_admission_tick?;
        self.next_admission_tick = admission_tick.checked_add(1);
        Some(admission_tick)
    }

    fn remember_failure(&mut self, failure: EditorImageHydrationFailure) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
    }
}

fn failure(
    asset_path: AssetPath,
    kind: EditorImageHydrationFailureKind,
) -> EditorImageHydrationFailure {
    EditorImageHydrationFailure { asset_path, kind }
}
