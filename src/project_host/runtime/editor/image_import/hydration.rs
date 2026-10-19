use std::collections::VecDeque;

use nara_asset::{AssetPath, AssetRecord, StableAssetId};
use nara_fs::{ContentDigest, FileIdentity, RelativePath};
use nara_image::{ImageImportSettings, ImageImportedAsset};

use crate::project_host::authoring::{
    AuthoringProjectAttemptGeneration, AuthoringProjectSessionSnapshot,
};
use crate::project_host::runtime::editor::authoring_runtime::{
    EditorAuthoringRuntime, EditorImageRuntimeFailure, EditorImageRuntimeTask,
    EditorImageRuntimeTerminal, EditorImageSourceCompletion,
};

use super::store::{
    EditorImageLastGood, EditorImageLastGoodStore, EditorImageSourceProof, MAX_LAST_GOOD_IMAGES,
};

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

struct EditorImageHydrationSource {
    attempt_generation: AuthoringProjectAttemptGeneration,
    project_root_identity: FileIdentity,
    record: AssetRecord,
    source_path: RelativePath,
    source_identity: FileIdentity,
    source_digest: ContentDigest,
    metadata_path: RelativePath,
    metadata_identity: FileIdentity,
    metadata_digest: ContentDigest,
    settings: ImageImportSettings,
}

impl EditorImageHydrationSource {
    #[must_use]
    fn asset_path(&self) -> &AssetPath {
        self.record.path()
    }

    #[must_use]
    fn stable_id(&self) -> StableAssetId {
        self.record.stable_id()
    }
}

struct PendingImageHydration {
    source: EditorImageHydrationSource,
    task: EditorImageRuntimeTask,
}

#[derive(Default)]
pub(super) struct EditorImageHydrationHost {
    initialized_attempt: Option<AuthoringProjectAttemptGeneration>,
    next_admission_tick: Option<u64>,
    queued: VecDeque<EditorImageHydrationSource>,
    pending: Option<PendingImageHydration>,
    failure: Option<EditorImageHydrationFailure>,
}

impl EditorImageHydrationHost {
    pub(super) fn initialize(&mut self, project: &AuthoringProjectSessionSnapshot) {
        let attempt_generation = project.attempt_generation();
        if self.initialized_attempt == Some(attempt_generation) {
            return;
        }
        self.cancel_pending();
        self.initialized_attempt = Some(attempt_generation);
        self.next_admission_tick = Some(attempt_generation.get());
        self.failure = None;
        for source in project.image_asset_sources() {
            let record = source.record();
            let metadata = source.metadata();
            let asset_path = record.path().clone();
            if self.queued.len() >= MAX_LAST_GOOD_IMAGES {
                self.remember_failure(failure(
                    asset_path,
                    EditorImageHydrationFailureKind::CapacityExceeded,
                ));
                break;
            }
            let settings = match metadata.admit::<ImageImportSettings>() {
                Ok(candidate) => *candidate.settings(),
                Err(_) => {
                    self.remember_failure(failure(
                        asset_path,
                        EditorImageHydrationFailureKind::ClosureInvalid,
                    ));
                    continue;
                }
            };
            let source_path = match RelativePath::new(source.source().project_path().as_str()) {
                Ok(path) => path,
                Err(_) => {
                    self.remember_failure(failure(
                        asset_path,
                        EditorImageHydrationFailureKind::ClosureInvalid,
                    ));
                    continue;
                }
            };
            let metadata_path =
                match RelativePath::new(source.metadata_source().project_path().as_str()) {
                    Ok(path) => path,
                    Err(_) => {
                        self.remember_failure(failure(
                            asset_path,
                            EditorImageHydrationFailureKind::ClosureInvalid,
                        ));
                        continue;
                    }
                };
            self.queued.push_back(EditorImageHydrationSource {
                attempt_generation,
                project_root_identity: project.project_root().identity(),
                record: record.clone(),
                source_path,
                source_identity: source.source().identity(),
                source_digest: source.source().digest(),
                metadata_path,
                metadata_identity: source.metadata_source().identity(),
                metadata_digest: source.metadata_source().digest(),
                settings,
            });
        }
    }

    #[must_use]
    pub(super) fn has_pending_work(&self) -> bool {
        self.pending.is_some() || !self.queued.is_empty()
    }

    pub(super) fn cancel_pending(&mut self) {
        self.pending = None;
        self.queued.clear();
    }

    pub(super) fn surrender_source(&mut self, asset_path: &AssetPath) {
        self.queued
            .retain(|source| source.asset_path() != asset_path);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.source.asset_path() == asset_path)
        {
            self.pending = None;
        }
    }

    pub(super) fn retain_catalog_eligible(
        &mut self,
        mut retain: impl FnMut(&AssetPath, StableAssetId, FileIdentity) -> bool,
    ) {
        self.queued.retain(|source| {
            retain(
                source.asset_path(),
                source.stable_id(),
                source.source_identity,
            )
        });
        if self.pending.as_ref().is_some_and(|pending| {
            !retain(
                pending.source.asset_path(),
                pending.source.stable_id(),
                pending.source.source_identity,
            )
        }) {
            self.pending = None;
        }
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
        let source_file = match project_root.open_file(&source.source_path) {
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
        self.pending = Some(PendingImageHydration { source, task });
    }

    fn advance_pending(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        runtime: &mut EditorAuthoringRuntime,
        store: &mut EditorImageLastGoodStore,
    ) -> bool {
        let terminal = {
            let Some(pending) = self.pending.as_mut() else {
                return false;
            };
            let Some(terminal) = EditorAuthoringRuntime::poll_image_import(&mut pending.task)
            else {
                return true;
            };
            terminal
        };
        let pending = self
            .pending
            .take()
            .expect("a terminal hydration task belongs to one pending source");
        let asset_path = pending.source.asset_path().clone();
        let result = match terminal {
            EditorImageRuntimeTerminal::Rejected(reason) => {
                Err(EditorImageHydrationFailureKind::Runtime(reason))
            }
            EditorImageRuntimeTerminal::Imported {
                handle,
                imported,
                source,
            } => self
                .validate_completion(project, &pending.source, source, &imported)
                .and_then(|()| {
                    if !store.can_insert(&asset_path) {
                        return Err(EditorImageHydrationFailureKind::CapacityExceeded);
                    }
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
                        EditorImageSourceProof::AuthoringClosure {
                            attempt_generation: pending.source.attempt_generation,
                        },
                        source_digest,
                        settings_hash,
                        version,
                    );
                    if store.insert(asset_path.clone(), last_good) {
                        Ok(())
                    } else {
                        Err(EditorImageHydrationFailureKind::CapacityExceeded)
                    }
                }),
        };
        if let Err(kind) = result {
            self.remember_failure(failure(asset_path, kind));
        }
        true
    }

    fn validate_completion(
        &self,
        project: &AuthoringProjectSessionSnapshot,
        expected: &EditorImageHydrationSource,
        completed_source: EditorImageSourceCompletion,
        imported: &ImageImportedAsset,
    ) -> Result<(), EditorImageHydrationFailureKind> {
        let project_root = project.project_root();
        if !self.current_authoring_attempt_matches(project, expected) {
            return Err(EditorImageHydrationFailureKind::SourceChanged);
        }
        if completed_source.identity() != expected.source_identity
            || completed_source.digest() != expected.source_digest
            || imported.source_digest() != expected.source_digest
        {
            return Err(EditorImageHydrationFailureKind::SourceChanged);
        }
        let metadata = project_root
            .open_file(&expected.metadata_path)
            .map_err(|_| EditorImageHydrationFailureKind::MetadataUnavailable)?;
        if metadata.identity() != expected.metadata_identity
            || metadata
                .verify_digest(expected.metadata_digest, super::metadata_byte_limit())
                .is_err()
        {
            return Err(EditorImageHydrationFailureKind::MetadataChanged);
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
