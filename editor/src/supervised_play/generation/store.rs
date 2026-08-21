use std::{
    fmt::{self, Debug, Formatter},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use nara::fs::{
    CapabilityRights, ConflictProtection, ContentDigest, DirectoryCapability,
    DirectoryPublicationBeginError, DirectoryPublicationOutcome, FileCapability,
    HostCapabilityOptions, LockMode, PublicationAtomicity, RelativeComponent, TrustMode,
};
use thiserror::Error;

#[cfg(test)]
use super::format::{
    GenerationBuildObservation, GenerationPackageFacts, GenerationSourceFacts,
    GenerationTargetFacts,
};
use super::{
    format::{
        ExecutableGenerationId, GenerationManifestInput, MAX_LAUNCH_MANIFEST_BYTES,
        encode_launch_manifest, generation_id,
    },
    platform::{EXECUTABLE_NAME, artifact_is_executable, open_directory_handle, open_regular_file},
};

const STORE_NAMESPACE: &str = "nara-editor-supervised-play-v1";
const OWNER_MARKER_NAME: &str = "owner.v1";
const OWNER_MARKER_MAGIC: &str = "NARA_EDITOR_SUPERVISED_PLAY_OWNER_V1";
const LAUNCH_MANIFEST_NAME: &str = "launch.manifest";

const MAX_EXECUTABLE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_OWNER_MARKER_BYTES: u64 = 256;

const SCAVENGE_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SCANNED_STORES: usize = 32;
const MAX_REMOVED_STORES: usize = 2;
const MAX_GENERATION_DIRECTORIES_PER_STORE: usize = 4;
const MAX_GENERATION_FILES_PER_DIRECTORY: usize = 2;
const MAX_SCAVENGE_STORE_BYTES: u64 = 6 * 1024 * 1024 * 1024;
const MAX_SCAVENGE_TOTAL_BYTES: u64 = 12 * 1024 * 1024 * 1024;

static NEXT_STORE_NONCE: AtomicU64 = AtomicU64::new(1);
static NEXT_STAGING_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(crate) struct ImmutableExecutableGeneration {
    id: ExecutableGenerationId,
    directory: PathBuf,
    executable: PathBuf,
    launch_manifest: PathBuf,
    executable_digest: ContentDigest,
    launch_manifest_digest: ContentDigest,
    source_receipt_digest: ContentDigest,
}

impl ImmutableExecutableGeneration {
    #[must_use]
    pub(crate) const fn id(&self) -> ExecutableGenerationId {
        self.id
    }

    #[must_use]
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    #[must_use]
    pub(crate) fn launch_manifest(&self) -> &Path {
        &self.launch_manifest
    }

    #[must_use]
    pub(crate) const fn executable_digest(&self) -> ContentDigest {
        self.executable_digest
    }

    #[must_use]
    pub(crate) const fn launch_manifest_digest(&self) -> ContentDigest {
        self.launch_manifest_digest
    }

    #[must_use]
    pub(crate) const fn source_receipt_digest(&self) -> ContentDigest {
        self.source_receipt_digest
    }
}

impl Debug for ImmutableExecutableGeneration {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImmutableExecutableGeneration")
            .field("id", &self.id)
            .field("executable_digest", &self.executable_digest)
            .field("launch_manifest_digest", &self.launch_manifest_digest)
            .field("source_receipt_digest", &self.source_receipt_digest)
            .finish_non_exhaustive()
    }
}

pub(crate) struct ExecutableGenerationStore {
    namespace: PathBuf,
    store_path: PathBuf,
    store_id: String,
    root: Option<DirectoryCapability>,
    owner_marker: Option<OwnerMarker>,
    next_generation: u64,
    active: Option<ImmutableExecutableGeneration>,
    last_good: Option<ImmutableExecutableGeneration>,
    candidate: Option<ImmutableExecutableGeneration>,
    cleanup_incomplete: bool,
    closed: bool,
}

impl ExecutableGenerationStore {
    pub(crate) fn create(base_root: &Path) -> Result<Self, GenerationStoreError> {
        Self::create_at(base_root, SystemTime::now())
    }

    fn create_at(base_root: &Path, now: SystemTime) -> Result<Self, GenerationStoreError> {
        let namespace = ensure_namespace(base_root)?;
        let created_unix_millis = unix_millis(now)?;
        let pid = std::process::id();

        for _ in 0..32 {
            let nonce = NEXT_STORE_NONCE.fetch_add(1, Ordering::Relaxed);
            let store_id = format!("run-{created_unix_millis:016x}-{pid:08x}-{nonce:016x}");
            let store_path = namespace.join(&store_id);
            match fs::create_dir(&store_path) {
                Ok(()) => {
                    let root = match open_directory_capability(&store_path) {
                        Ok(root) => root,
                        Err(error) => {
                            let _ = fs::remove_dir_all(&store_path);
                            return Err(error);
                        }
                    };
                    let marker = match OwnerMarker::create(&root, &store_id, created_unix_millis) {
                        Ok(marker) => marker,
                        Err(error) => {
                            drop(root);
                            let _ = fs::remove_dir_all(&store_path);
                            return Err(error);
                        }
                    };
                    return Ok(Self {
                        namespace,
                        store_path,
                        store_id,
                        root: Some(root),
                        owner_marker: Some(marker),
                        next_generation: 1,
                        active: None,
                        last_good: None,
                        candidate: None,
                        cleanup_incomplete: false,
                        closed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => {
                    return Err(GenerationStoreError::Io {
                        operation: "create-run-store",
                        source,
                    });
                }
            }
        }

        Err(GenerationStoreError::RunStoreNameExhausted)
    }

    pub(crate) fn scavenge_inactive(
        base_root: &Path,
    ) -> Result<GenerationScavengeReceipt, GenerationStoreError> {
        Self::scavenge_inactive_at(base_root, SystemTime::now())
    }

    fn scavenge_inactive_at(
        base_root: &Path,
        now: SystemTime,
    ) -> Result<GenerationScavengeReceipt, GenerationStoreError> {
        let namespace = ensure_namespace(base_root)?;
        let now_millis = unix_millis(now)?;
        let minimum_age_millis = u64::try_from(SCAVENGE_MIN_AGE.as_millis())
            .map_err(|_| GenerationStoreError::ClockOutOfRange)?;
        let oldest_allowed = now_millis.saturating_sub(minimum_age_millis);
        let mut receipt = GenerationScavengeReceipt::default();
        let entries = fs::read_dir(&namespace).map_err(|source| GenerationStoreError::Io {
            operation: "enumerate-store-namespace",
            source,
        })?;

        for entry in entries {
            if receipt.examined >= MAX_SCANNED_STORES {
                receipt.limit_reached = true;
                break;
            }
            receipt.examined += 1;
            let Ok(entry) = entry else {
                receipt.skipped_unverified += 1;
                continue;
            };
            let Some(store_id) = entry.file_name().to_str().map(str::to_owned) else {
                receipt.skipped_unverified += 1;
                continue;
            };
            if !valid_store_id(&store_id) {
                receipt.skipped_unverified += 1;
                continue;
            }
            let store_path = entry.path();
            let Some(marker) = inspect_inactive_marker(&store_path, &store_id, oldest_allowed)
            else {
                receipt.skipped_unverified += 1;
                continue;
            };
            let marker = match marker {
                InactiveMarkerObservation::Live => {
                    receipt.skipped_live += 1;
                    continue;
                }
                InactiveMarkerObservation::Inactive(marker) => marker,
            };
            let Some(store_bytes) = measure_valid_store_tree(&store_path) else {
                receipt.skipped_unverified += 1;
                continue;
            };
            if !marker.named_binding_is_current() {
                receipt.skipped_unverified += 1;
                continue;
            }
            let Some(next_total) = receipt.removed_bytes.checked_add(store_bytes) else {
                receipt.limit_reached = true;
                break;
            };
            if receipt.removed >= MAX_REMOVED_STORES || next_total > MAX_SCAVENGE_TOTAL_BYTES {
                receipt.limit_reached = true;
                break;
            }

            match fs::remove_dir_all(&store_path) {
                Ok(()) => {
                    receipt.removed += 1;
                    receipt.removed_bytes = next_total;
                }
                Err(_) => receipt.cleanup_failures += 1,
            }
            drop(marker);
        }

        Ok(receipt)
    }

    pub(crate) fn publish_candidate(
        &mut self,
        artifact_path: &Path,
        manifest: GenerationManifestInput,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        self.ensure_operational()?;
        if !artifact_path.is_absolute() {
            return Err(GenerationStoreError::ArtifactPathNotAbsolute);
        }
        self.evict_candidate()?;

        let source = open_artifact(artifact_path)?;
        let executable_digest = source.digest(MAX_EXECUTABLE_BYTES).map_err(|source| {
            GenerationStoreError::Capability {
                operation: "digest-source-executable",
                source,
            }
        })?;
        let sequence = self.next_generation;
        let next_generation = sequence
            .checked_add(1)
            .ok_or(GenerationStoreError::GenerationSequenceExhausted)?;
        let generation_id = generation_id(&self.store_id, sequence, executable_digest);
        let manifest_bytes = encode_launch_manifest(generation_id, executable_digest, &manifest)?;
        let manifest_digest = ContentDigest::of_bytes(&manifest_bytes);
        let source_receipt_digest = manifest.source_receipt_digest();
        let destination_name = format!("generation-{}", generation_id.hex());
        let staging_nonce = NEXT_STAGING_NONCE.fetch_add(1, Ordering::Relaxed);
        let staging_name = format!(
            ".generation-{}.stage-{staging_nonce:016x}",
            generation_id.hex()
        );
        let destination_component = component(&destination_name)?;
        let staging_component = component(&staging_name)?;
        let root = self
            .root
            .as_ref()
            .ok_or(GenerationStoreError::StoreClosed)?;
        let transaction =
            match root.begin_directory_publication(&destination_component, &staging_component) {
                Ok(transaction) => transaction,
                Err(DirectoryPublicationBeginError::Orphaned(orphan)) => {
                    drop(orphan);
                    if !remove_exact_generation_tree(&self.store_path, &staging_name) {
                        self.cleanup_incomplete = true;
                        return Err(GenerationStoreError::StagingCleanupIncomplete);
                    }
                    return Err(GenerationStoreError::PublicationBeginOrphaned);
                }
                Err(error) => return Err(GenerationStoreError::PublicationBegin(error)),
            };

        let staging = transaction.staging_directory();
        let populate_result = populate_generation(
            &staging,
            &source,
            executable_digest,
            &manifest_bytes,
            manifest_digest,
        );
        drop(staging);
        if let Err(error) = populate_result {
            let outcome = transaction.abort();
            drop(outcome);
            if !remove_exact_generation_tree(&self.store_path, &staging_name) {
                self.cleanup_incomplete = true;
                return Err(GenerationStoreError::StagingCleanupIncomplete);
            }
            return Err(error);
        }

        match transaction.publish() {
            DirectoryPublicationOutcome::Published(receipt)
                if receipt.publication_atomicity() == PublicationAtomicity::AtomicNameSwitch
                    && receipt.conflict_protection()
                        == ConflictProtection::StrongCompareAndSwap => {}
            DirectoryPublicationOutcome::Published(_) => {
                if !remove_exact_generation_tree(&self.store_path, &destination_name) {
                    self.cleanup_incomplete = true;
                    return Err(GenerationStoreError::PublishedCleanupIncomplete);
                }
                return Err(GenerationStoreError::PublicationProofInsufficient);
            }
            outcome => {
                drop(outcome);
                if !remove_exact_generation_tree(&self.store_path, &staging_name) {
                    self.cleanup_incomplete = true;
                    return Err(GenerationStoreError::StagingCleanupIncomplete);
                }
                return Err(GenerationStoreError::PublicationRejected);
            }
        }

        let directory = self.store_path.join(&destination_name);
        let executable = directory.join(EXECUTABLE_NAME);
        let launch_manifest = directory.join(LAUNCH_MANIFEST_NAME);
        let verification = verify_published_file(
            &executable,
            executable_digest,
            MAX_EXECUTABLE_BYTES,
            "verify-published-executable",
        )
        .and_then(|()| {
            verify_published_file(
                &launch_manifest,
                manifest_digest,
                MAX_LAUNCH_MANIFEST_BYTES as u64,
                "verify-published-launch-manifest",
            )
        });
        if let Err(error) = verification {
            if !remove_exact_generation_tree(&self.store_path, &destination_name) {
                self.cleanup_incomplete = true;
                return Err(GenerationStoreError::PublishedCleanupIncomplete);
            }
            return Err(error);
        }

        let generation = ImmutableExecutableGeneration {
            id: generation_id,
            directory,
            executable,
            launch_manifest,
            executable_digest,
            launch_manifest_digest: manifest_digest,
            source_receipt_digest,
        };
        self.next_generation = next_generation;
        self.candidate = Some(generation.clone());
        Ok(generation)
    }

    pub(crate) fn activate_candidate(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        self.ensure_operational()?;
        if self.active.is_some() {
            return Err(GenerationStoreError::ActiveGenerationExists);
        }
        let candidate = self
            .candidate
            .as_ref()
            .filter(|candidate| candidate.id == generation)
            .cloned()
            .ok_or(GenerationStoreError::CandidateMismatch)?;
        self.candidate = None;
        self.active = Some(candidate.clone());
        Ok(candidate)
    }

    pub(crate) fn activate_last_good(
        &mut self,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        self.ensure_operational()?;
        if self.active.is_some() {
            return Err(GenerationStoreError::ActiveGenerationExists);
        }
        let generation = self
            .last_good
            .clone()
            .ok_or(GenerationStoreError::LastGoodUnavailable)?;
        self.active = Some(generation.clone());
        Ok(generation)
    }

    pub(crate) fn admit_ready(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<(), GenerationStoreError> {
        self.ensure_operational()?;
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == generation)
            .cloned()
            .ok_or(GenerationStoreError::GenerationNotActive)?;
        if self
            .last_good
            .as_ref()
            .is_some_and(|value| value.id == generation)
        {
            return Ok(());
        }
        if let Some(previous) = self.last_good.as_ref()
            && !self.is_retained_elsewhere(previous.id, RetentionSlot::LastGood)
        {
            remove_generation(&self.store_path, previous)?;
        }
        self.last_good = Some(active);
        Ok(())
    }

    pub(crate) fn finish_active(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<(), GenerationStoreError> {
        self.ensure_operational()?;
        let active = self
            .active
            .as_ref()
            .filter(|active| active.id == generation)
            .ok_or(GenerationStoreError::GenerationNotActive)?;
        if !self.is_retained_elsewhere(active.id, RetentionSlot::Active) {
            remove_generation(&self.store_path, active)?;
        }
        self.active = None;
        Ok(())
    }

    pub(crate) fn discard_candidate(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<(), GenerationStoreError> {
        self.ensure_operational()?;
        let candidate = self
            .candidate
            .as_ref()
            .filter(|candidate| candidate.id == generation)
            .ok_or(GenerationStoreError::CandidateMismatch)?;
        remove_generation(&self.store_path, candidate)?;
        self.candidate = None;
        Ok(())
    }

    #[must_use]
    pub(crate) fn active(&self) -> Option<&ImmutableExecutableGeneration> {
        self.active.as_ref()
    }

    #[must_use]
    pub(crate) fn last_good(&self) -> Option<&ImmutableExecutableGeneration> {
        self.last_good.as_ref()
    }

    #[must_use]
    pub(crate) fn candidate(&self) -> Option<&ImmutableExecutableGeneration> {
        self.candidate.as_ref()
    }

    pub(crate) fn try_close(
        &mut self,
    ) -> Result<GenerationStoreCloseReceipt, GenerationStoreError> {
        if self.closed {
            return Err(GenerationStoreError::StoreClosed);
        }
        if self.active.is_some() {
            return Err(GenerationStoreError::ActiveGenerationExists);
        }
        let owner_marker = self
            .owner_marker
            .as_ref()
            .ok_or(GenerationStoreError::StoreClosed)?;
        validate_owned_store(
            &self.namespace,
            &self.store_path,
            &self.store_id,
            owner_marker,
        )?;
        fs::remove_dir_all(&self.store_path).map_err(|source| GenerationStoreError::Io {
            operation: "remove-run-store",
            source,
        })?;
        self.candidate = None;
        self.last_good = None;
        self.root = None;
        self.owner_marker = None;
        self.closed = true;
        Ok(GenerationStoreCloseReceipt { removed: true })
    }

    fn evict_candidate(&mut self) -> Result<(), GenerationStoreError> {
        let Some(candidate) = self.candidate.as_ref() else {
            return Ok(());
        };
        remove_generation(&self.store_path, candidate)?;
        self.candidate = None;
        Ok(())
    }

    fn is_retained_elsewhere(
        &self,
        generation: ExecutableGenerationId,
        excluding: RetentionSlot,
    ) -> bool {
        (excluding != RetentionSlot::Active
            && self
                .active
                .as_ref()
                .is_some_and(|value| value.id == generation))
            || (excluding != RetentionSlot::LastGood
                && self
                    .last_good
                    .as_ref()
                    .is_some_and(|value| value.id == generation))
            || (excluding != RetentionSlot::Candidate
                && self
                    .candidate
                    .as_ref()
                    .is_some_and(|value| value.id == generation))
    }

    fn ensure_operational(&self) -> Result<(), GenerationStoreError> {
        if self.closed {
            Err(GenerationStoreError::StoreClosed)
        } else if self.cleanup_incomplete {
            Err(GenerationStoreError::CleanupIncomplete)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetentionSlot {
    Active,
    LastGood,
    Candidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GenerationStoreCloseReceipt {
    removed: bool,
}

impl GenerationStoreCloseReceipt {
    #[must_use]
    pub(crate) const fn store_removed(self) -> bool {
        self.removed
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GenerationScavengeReceipt {
    examined: usize,
    removed: usize,
    removed_bytes: u64,
    skipped_live: usize,
    skipped_unverified: usize,
    cleanup_failures: usize,
    limit_reached: bool,
}

impl GenerationScavengeReceipt {
    #[must_use]
    pub(crate) const fn examined(self) -> usize {
        self.examined
    }

    #[must_use]
    pub(crate) const fn removed(self) -> usize {
        self.removed
    }

    #[must_use]
    pub(crate) const fn removed_bytes(self) -> u64 {
        self.removed_bytes
    }

    #[must_use]
    pub(crate) const fn skipped_live(self) -> usize {
        self.skipped_live
    }

    #[must_use]
    pub(crate) const fn skipped_unverified(self) -> usize {
        self.skipped_unverified
    }

    #[must_use]
    pub(crate) const fn cleanup_failures(self) -> usize {
        self.cleanup_failures
    }

    #[must_use]
    pub(crate) const fn limit_reached(self) -> bool {
        self.limit_reached
    }
}

struct OwnerMarker {
    _file: FileCapability,
    root: DirectoryCapability,
}

impl OwnerMarker {
    fn create(
        root: &DirectoryCapability,
        store_id: &str,
        created_unix_millis: u64,
    ) -> Result<Self, GenerationStoreError> {
        let bytes = owner_marker_bytes(store_id, created_unix_millis);
        let marker_name = component(OWNER_MARKER_NAME)?;
        let mut file = root.create_child_file(&marker_name).map_err(|source| {
            GenerationStoreError::Capability {
                operation: "create-owner-marker",
                source,
            }
        })?;
        let mut writer = file
            .writer()
            .map_err(|source| GenerationStoreError::Capability {
                operation: "write-owner-marker",
                source,
            })?;
        writer
            .write_all(&bytes)
            .and_then(|()| writer.flush())
            .map_err(|source| GenerationStoreError::Io {
                operation: "write-owner-marker",
                source,
            })?;
        drop(writer);
        file.sync()
            .map_err(|source| GenerationStoreError::Capability {
                operation: "sync-owner-marker",
                source,
            })?;
        retain_exclusive_lock(&file)?;
        Ok(Self {
            _file: file,
            root: root.share(),
        })
    }

    fn read_bounded(&self) -> Result<Vec<u8>, GenerationStoreError> {
        self._file
            .read_to_end_bounded(MAX_OWNER_MARKER_BYTES)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "read-owned-owner-marker",
                source,
            })
    }

    fn named_binding_is_current(&self) -> bool {
        let Ok(marker_name) = component(OWNER_MARKER_NAME) else {
            return false;
        };
        let Ok(named) = self.root.open_child_file(&marker_name) else {
            return false;
        };
        self._file.identity() == named.identity()
            && self._file.parent_identity() == Some(self.root.identity())
            && named.parent_identity() == Some(self.root.identity())
            && self._file.link_count() == 1
            && named.link_count() == 1
            && matches!(
                named.try_lock(LockMode::Exclusive),
                Err(nara::fs::FsError::LockContended)
            )
    }
}

enum InactiveMarkerObservation {
    Live,
    Inactive(OwnerMarker),
}

fn inspect_inactive_marker(
    store_path: &Path,
    expected_store_id: &str,
    oldest_allowed: u64,
) -> Option<InactiveMarkerObservation> {
    let root = open_directory_capability(store_path).ok()?;
    let marker_name = component(OWNER_MARKER_NAME).ok()?;
    let capability = root.open_child_file(&marker_name).ok()?;
    match retain_exclusive_lock(&capability) {
        Ok(()) => {
            let bytes = capability
                .read_to_end_bounded(MAX_OWNER_MARKER_BYTES)
                .ok()?;
            let facts = parse_owner_marker(&bytes)?;
            if facts.store_id != expected_store_id || facts.created_unix_millis > oldest_allowed {
                return None;
            }
            Some(InactiveMarkerObservation::Inactive(OwnerMarker {
                _file: capability,
                root,
            }))
        }
        Err(GenerationStoreError::Capability {
            source: nara::fs::FsError::LockContended,
            ..
        }) => Some(InactiveMarkerObservation::Live),
        Err(_) => None,
    }
}

fn retain_exclusive_lock(file: &FileCapability) -> Result<(), GenerationStoreError> {
    let lock =
        file.try_lock(LockMode::Exclusive)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "lock-owner-marker",
                source,
            })?;
    // The operating-system lock is tied to the opened file. Keeping the capability alive while
    // forgetting this unlocking guard intentionally retains ownership until that file closes.
    std::mem::forget(lock);
    Ok(())
}

fn ensure_namespace(base_root: &Path) -> Result<PathBuf, GenerationStoreError> {
    fs::create_dir_all(base_root).map_err(|source| GenerationStoreError::Io {
        operation: "create-store-base",
        source,
    })?;
    let namespace = base_root.join(STORE_NAMESPACE);
    fs::create_dir_all(&namespace).map_err(|source| GenerationStoreError::Io {
        operation: "create-store-namespace",
        source,
    })?;
    let metadata = fs::symlink_metadata(&namespace).map_err(|source| GenerationStoreError::Io {
        operation: "inspect-store-namespace",
        source,
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(GenerationStoreError::NamespaceNotPlainDirectory);
    }
    fs::canonicalize(namespace).map_err(|source| GenerationStoreError::Io {
        operation: "canonicalize-store-namespace",
        source,
    })
}

fn open_directory_capability(path: &Path) -> Result<DirectoryCapability, GenerationStoreError> {
    let file = open_directory_handle(path).map_err(|source| GenerationStoreError::Io {
        operation: "open-run-store",
        source,
    })?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .map_err(|source| GenerationStoreError::Capability {
        operation: "admit-run-store",
        source,
    })
}

fn open_artifact(path: &Path) -> Result<FileCapability, GenerationStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| GenerationStoreError::Io {
        operation: "inspect-source-executable",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(GenerationStoreError::ArtifactNotPlainFile);
    }
    if !artifact_is_executable(path, &metadata) {
        return Err(GenerationStoreError::ArtifactNotExecutable);
    }
    let file = open_regular_file(path).map_err(|source| GenerationStoreError::Io {
        operation: "open-source-executable",
        source,
    })?;
    FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0).map_err(|source| {
        GenerationStoreError::Capability {
            operation: "admit-source-executable",
            source,
        }
    })
}

fn populate_generation(
    staging: &DirectoryCapability,
    source: &FileCapability,
    expected_executable_digest: ContentDigest,
    manifest_bytes: &[u8],
    expected_manifest_digest: ContentDigest,
) -> Result<(), GenerationStoreError> {
    let executable_name = component(EXECUTABLE_NAME)?;
    let mut executable = staging
        .create_child_executable_file(&executable_name)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "create-generation-executable",
            source,
        })?;
    copy_source_executable(source, &mut executable, expected_executable_digest)?;
    executable
        .sync()
        .map_err(|source| GenerationStoreError::Capability {
            operation: "sync-generation-executable",
            source,
        })?;
    executable
        .verify_digest(expected_executable_digest, MAX_EXECUTABLE_BYTES)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "verify-generation-executable",
            source,
        })?;
    let source_after =
        source
            .digest(MAX_EXECUTABLE_BYTES)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "redigest-source-executable",
                source,
            })?;
    if source_after != expected_executable_digest {
        return Err(GenerationStoreError::ArtifactChangedDuringCopy);
    }

    let manifest_name = component(LAUNCH_MANIFEST_NAME)?;
    let mut manifest = staging
        .create_child_file(&manifest_name)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "create-generation-launch-manifest",
            source,
        })?;
    {
        let mut writer = manifest
            .writer()
            .map_err(|source| GenerationStoreError::Capability {
                operation: "open-generation-launch-manifest-writer",
                source,
            })?;
        writer
            .write_all(manifest_bytes)
            .and_then(|()| writer.flush())
            .map_err(|source| GenerationStoreError::Io {
                operation: "write-generation-launch-manifest",
                source,
            })?;
    }
    manifest
        .sync()
        .map_err(|source| GenerationStoreError::Capability {
            operation: "sync-generation-launch-manifest",
            source,
        })?;
    manifest
        .verify_digest(expected_manifest_digest, MAX_LAUNCH_MANIFEST_BYTES as u64)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "verify-generation-launch-manifest",
            source,
        })?;
    Ok(())
}

fn copy_source_executable(
    source: &FileCapability,
    destination: &mut FileCapability,
    expected: ContentDigest,
) -> Result<(), GenerationStoreError> {
    let mut reader = source
        .reader()
        .map_err(|source| GenerationStoreError::Capability {
            operation: "open-source-executable-reader",
            source,
        })?;
    let mut writer = destination
        .writer()
        .map_err(|source| GenerationStoreError::Capability {
            operation: "open-generation-executable-writer",
            source,
        })?;
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| GenerationStoreError::Io {
                operation: "read-source-executable",
                source,
            })?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read as u64)
            .ok_or(GenerationStoreError::ArtifactTooLarge)?;
        if copied > expected.length() || copied > MAX_EXECUTABLE_BYTES {
            return Err(GenerationStoreError::ArtifactChangedDuringCopy);
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|source| GenerationStoreError::Io {
                operation: "write-generation-executable",
                source,
            })?;
    }
    writer.flush().map_err(|source| GenerationStoreError::Io {
        operation: "flush-generation-executable",
        source,
    })?;
    if copied != expected.length() {
        return Err(GenerationStoreError::ArtifactChangedDuringCopy);
    }
    Ok(())
}

fn verify_published_file(
    path: &Path,
    expected: ContentDigest,
    limit: u64,
    operation: &'static str,
) -> Result<(), GenerationStoreError> {
    let file =
        open_regular_file(path).map_err(|source| GenerationStoreError::Io { operation, source })?;
    let file = FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0)
        .map_err(|source| GenerationStoreError::Capability { operation, source })?;
    file.verify_digest(expected, limit)
        .map_err(|source| GenerationStoreError::Capability { operation, source })
}

fn remove_generation(
    store_path: &Path,
    generation: &ImmutableExecutableGeneration,
) -> Result<(), GenerationStoreError> {
    if remove_exact_generation_tree(
        store_path,
        generation
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(GenerationStoreError::GenerationPathInvalid)?,
    ) {
        Ok(())
    } else {
        Err(GenerationStoreError::GenerationCleanupIncomplete)
    }
}

fn remove_exact_generation_tree(store_path: &Path, child_name: &str) -> bool {
    if !valid_generation_directory_name(child_name) {
        return false;
    }
    let child = store_path.join(child_name);
    if child.parent() != Some(store_path) {
        return false;
    }
    match fs::symlink_metadata(&child) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(child).is_ok()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        _ => false,
    }
}

fn validate_owned_store(
    namespace: &Path,
    store_path: &Path,
    expected_store_id: &str,
    owner_marker: &OwnerMarker,
) -> Result<(), GenerationStoreError> {
    if store_path.parent() != Some(namespace)
        || store_path.file_name().and_then(|name| name.to_str()) != Some(expected_store_id)
        || !valid_store_id(expected_store_id)
    {
        return Err(GenerationStoreError::StoreIdentityInvalid);
    }
    let marker = owner_marker.read_bounded()?;
    if marker.len() as u64 > MAX_OWNER_MARKER_BYTES
        || parse_owner_marker(&marker).is_none_or(|facts| facts.store_id != expected_store_id)
        || !owner_marker.named_binding_is_current()
        || measure_valid_store_tree(store_path).is_none()
    {
        return Err(GenerationStoreError::StoreIdentityInvalid);
    }
    Ok(())
}

fn measure_valid_store_tree(store_path: &Path) -> Option<u64> {
    let metadata = fs::symlink_metadata(store_path).ok()?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return None;
    }
    let mut total = 0_u64;
    let mut generation_directories = 0_usize;
    for entry in fs::read_dir(store_path).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_str()?;
        let metadata = fs::symlink_metadata(entry.path()).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
        if name == OWNER_MARKER_NAME && metadata.file_type().is_file() {
            if metadata.len() > MAX_OWNER_MARKER_BYTES {
                return None;
            }
            total = total.checked_add(metadata.len())?;
            continue;
        }
        if !valid_generation_directory_name(name) || !metadata.file_type().is_dir() {
            return None;
        }
        generation_directories = generation_directories.checked_add(1)?;
        if generation_directories > MAX_GENERATION_DIRECTORIES_PER_STORE {
            return None;
        }
        let mut files = 0_usize;
        for child in fs::read_dir(entry.path()).ok()? {
            let child = child.ok()?;
            let child_name = child.file_name();
            let child_name = child_name.to_str()?;
            let child_metadata = fs::symlink_metadata(child.path()).ok()?;
            if child_metadata.file_type().is_symlink() || !child_metadata.file_type().is_file() {
                return None;
            }
            files = files.checked_add(1)?;
            if files > MAX_GENERATION_FILES_PER_DIRECTORY {
                return None;
            }
            match child_name {
                EXECUTABLE_NAME if child_metadata.len() <= MAX_EXECUTABLE_BYTES => {}
                LAUNCH_MANIFEST_NAME
                    if child_metadata.len() <= MAX_LAUNCH_MANIFEST_BYTES as u64 => {}
                _ => return None,
            }
            total = total.checked_add(child_metadata.len())?;
            if total > MAX_SCAVENGE_STORE_BYTES {
                return None;
            }
        }
    }
    Some(total)
}

struct OwnerMarkerFacts<'a> {
    store_id: &'a str,
    created_unix_millis: u64,
}

fn owner_marker_bytes(store_id: &str, created_unix_millis: u64) -> Vec<u8> {
    format!("{OWNER_MARKER_MAGIC}\nstore_id={store_id}\ncreated_unix_ms={created_unix_millis}\n")
        .into_bytes()
}

fn parse_owner_marker(bytes: &[u8]) -> Option<OwnerMarkerFacts<'_>> {
    if bytes.len() as u64 > MAX_OWNER_MARKER_BYTES {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split_terminator('\n');
    if lines.next()? != OWNER_MARKER_MAGIC {
        return None;
    }
    let store_id = lines.next()?.strip_prefix("store_id=")?;
    let created_unix_millis = lines
        .next()?
        .strip_prefix("created_unix_ms=")?
        .parse()
        .ok()?;
    if lines.next().is_some() || !text.ends_with('\n') || !valid_store_id(store_id) {
        return None;
    }
    Some(OwnerMarkerFacts {
        store_id,
        created_unix_millis,
    })
}

fn valid_store_id(value: &str) -> bool {
    let mut parts = value.split('-');
    matches!(parts.next(), Some("run"))
        && parts.next().is_some_and(|part| is_lower_hex(part, 16))
        && parts.next().is_some_and(|part| is_lower_hex(part, 8))
        && parts.next().is_some_and(|part| is_lower_hex(part, 16))
        && parts.next().is_none()
}

fn valid_generation_directory_name(value: &str) -> bool {
    if let Some(id) = value.strip_prefix("generation-") {
        return is_lower_hex(id, 64);
    }
    let Some(rest) = value.strip_prefix(".generation-") else {
        return false;
    };
    let Some((id, nonce)) = rest.split_once(".stage-") else {
        return false;
    };
    is_lower_hex(id, 64) && is_lower_hex(nonce, 16)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn component(value: &str) -> Result<RelativeComponent, GenerationStoreError> {
    RelativeComponent::new(value).map_err(|_| GenerationStoreError::InternalNameRejected)
}

fn unix_millis(time: SystemTime) -> Result<u64, GenerationStoreError> {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| GenerationStoreError::ClockBeforeEpoch)?
        .as_millis();
    u64::try_from(millis).map_err(|_| GenerationStoreError::ClockOutOfRange)
}

#[derive(Debug, Error)]
pub(crate) enum GenerationStoreError {
    #[error("generation artifact path must be absolute")]
    ArtifactPathNotAbsolute,
    #[error("generation artifact must be a plain regular file")]
    ArtifactNotPlainFile,
    #[error("generation artifact is not executable on this platform")]
    ArtifactNotExecutable,
    #[error("generation artifact exceeded the fixed byte limit")]
    ArtifactTooLarge,
    #[error("generation artifact changed while it was copied")]
    ArtifactChangedDuringCopy,
    #[error("the active executable generation must retire before another launch")]
    ActiveGenerationExists,
    #[error("the requested generation is not the current candidate")]
    CandidateMismatch,
    #[error("the requested generation is not active")]
    GenerationNotActive,
    #[error("no session last-good generation is available")]
    LastGoodUnavailable,
    #[error("the executable generation store is closed")]
    StoreClosed,
    #[error("the executable generation store has incomplete cleanup")]
    CleanupIncomplete,
    #[error("the generation store namespace is not a plain directory")]
    NamespaceNotPlainDirectory,
    #[error("the generation store owner identity could not be revalidated")]
    StoreIdentityInvalid,
    #[error("the generation path failed exact child validation")]
    GenerationPathInvalid,
    #[error("the run store name retry budget was exhausted")]
    RunStoreNameExhausted,
    #[error("the generation sequence was exhausted")]
    GenerationSequenceExhausted,
    #[error("the system clock precedes the Unix epoch")]
    ClockBeforeEpoch,
    #[error("the system clock cannot be represented by the generation store")]
    ClockOutOfRange,
    #[error("the source receipt exceeds the fixed launch-manifest limit")]
    SourceReceiptTooLarge,
    #[error("the source receipt must not be empty")]
    SourceReceiptEmpty,
    #[error("the launch manifest contains too many features")]
    TooManyFeatures,
    #[error("the selected manifest path is not a canonical relative path")]
    ManifestRelativePathInvalid,
    #[error("launch-manifest field `{field}` is empty")]
    ManifestTextEmpty { field: &'static str },
    #[error("launch-manifest field `{field}` exceeds {limit} bytes")]
    ManifestTextTooLarge { field: &'static str, limit: usize },
    #[error("the canonical launch manifest exceeds its fixed byte limit")]
    LaunchManifestTooLarge,
    #[error("a bounded generation allocation failed")]
    AllocationFailed,
    #[error("an internal generation namespace component was rejected")]
    InternalNameRejected,
    #[error("generation directory publication began with an orphaned staging object")]
    PublicationBeginOrphaned,
    #[error("generation directory publication was rejected")]
    PublicationRejected,
    #[error("generation directory publication did not provide atomic no-replace evidence")]
    PublicationProofInsufficient,
    #[error("generation staging cleanup is incomplete")]
    StagingCleanupIncomplete,
    #[error("published generation cleanup is incomplete")]
    PublishedCleanupIncomplete,
    #[error("generation retention cleanup is incomplete")]
    GenerationCleanupIncomplete,
    #[error("generation directory publication could not begin: {0}")]
    PublicationBegin(#[source] DirectoryPublicationBeginError),
    #[error("generation filesystem operation `{operation}` failed: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("generation capability operation `{operation}` failed: {source}")]
    Capability {
        operation: &'static str,
        #[source]
        source: nara::fs::FsError,
    },
}

#[cfg(test)]
#[path = "../generation_tests.rs"]
mod tests;
