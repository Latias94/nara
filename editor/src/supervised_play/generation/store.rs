mod publication;
mod retention;
mod scavenge;

use std::{
    fmt::{self, Debug, Formatter},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use nara::fs::{
    ContentDigest, DirectoryCapability, DirectoryPublicationBeginError, FileCapability,
    RelativeComponent,
};
use nara::project_host::{SupervisedDesktopProductIdentity, SupervisedDesktopWireError};
use thiserror::Error;

use crate::supervised_play::process::ExactGenerationLaunchAuthority;

use super::format::{ExecutableGenerationId, GenerationManifestInput};
#[cfg(test)]
use super::format::{
    GenerationBuildObservation, GenerationPackageFacts, GenerationSourceFacts,
    GenerationTargetFacts,
};
#[cfg(test)]
use super::platform::EXECUTABLE_NAME;
use publication::PublishedGenerationBinding;
#[cfg(test)]
pub(super) use scavenge::NEXT_STORE_NONCE;
use scavenge::OwnerMarker;
#[cfg(test)]
use scavenge::{
    MAX_SCAVENGE_WORK_DEPTH, SCAVENGE_MIN_AGE, STORE_NAMESPACE, measure_valid_store_tree,
};

const LAUNCH_MANIFEST_NAME: &str = "launch.manifest";
const WORK_DIRECTORY_PREFIX: &str = "work-";
const MAX_GENERATION_FILES_PER_DIRECTORY: usize = 2;

pub(crate) struct ImmutableExecutableGeneration {
    id: ExecutableGenerationId,
    sequence: u64,
    product_identity: SupervisedDesktopProductIdentity,
    binding: PublishedGenerationBinding,
    directory: PathBuf,
    executable: PathBuf,
    launch_manifest: PathBuf,
    executable_digest: ContentDigest,
    launch_manifest_digest: ContentDigest,
    source_receipt: Arc<Vec<u8>>,
}

impl ImmutableExecutableGeneration {
    #[must_use]
    pub(crate) const fn id(&self) -> ExecutableGenerationId {
        self.id
    }

    #[must_use]
    pub(crate) const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub(crate) const fn product_identity(&self) -> &SupervisedDesktopProductIdentity {
        &self.product_identity
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn launch_manifest(&self) -> &Path {
        &self.launch_manifest
    }

    pub(crate) fn revalidate_for_launch(
        &self,
    ) -> Result<ExecutableGenerationLaunchReceipt, GenerationStoreError> {
        let (directory, members) = self
            .binding
            .revalidate(self.executable_digest, self.launch_manifest_digest)?;
        let generation_directory =
            directory
                .duplicate_restrictive_host_handle()
                .map_err(|source| GenerationStoreError::Capability {
                    operation: "duplicate-generation-directory-launch-handle",
                    source,
                })?;
        let executable = members
            .executable
            .duplicate_restrictive_executable_host_handle()
            .map_err(|source| GenerationStoreError::Capability {
                operation: "duplicate-generation-executable-launch-handle",
                source,
            })?;
        let launch_manifest = members
            .launch_manifest
            .duplicate_restrictive_host_handle()
            .map_err(|source| GenerationStoreError::Capability {
                operation: "duplicate-generation-launch-manifest-handle",
                source,
            })?;
        let authority = ExactGenerationLaunchAuthority::from_verified_handles(
            executable,
            generation_directory,
            launch_manifest,
        )
        .map_err(|source| GenerationStoreError::Io {
            operation: "prepare-exact-generation-launch-authority",
            source,
        })?;

        Ok(ExecutableGenerationLaunchReceipt {
            authority,
            _directory: directory,
            _executable: members.executable,
            _launch_manifest: members.launch_manifest,
        })
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
    pub(crate) fn source_receipt(&self) -> &[u8] {
        self.source_receipt.as_slice()
    }
}

impl Clone for ImmutableExecutableGeneration {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            sequence: self.sequence,
            product_identity: self.product_identity.clone(),
            binding: self.binding.clone(),
            directory: self.directory.clone(),
            executable: self.executable.clone(),
            launch_manifest: self.launch_manifest.clone(),
            executable_digest: self.executable_digest,
            launch_manifest_digest: self.launch_manifest_digest,
            source_receipt: Arc::clone(&self.source_receipt),
        }
    }
}

impl Debug for ImmutableExecutableGeneration {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImmutableExecutableGeneration")
            .field("id", &self.id)
            .field("sequence", &self.sequence)
            .field("product_identity", &self.product_identity)
            .field("executable_digest", &self.executable_digest)
            .field("launch_manifest_digest", &self.launch_manifest_digest)
            .field("source_receipt_bytes", &self.source_receipt.len())
            .finish_non_exhaustive()
    }
}

#[must_use = "a revalidated generation must be consumed by the immediate spawn attempt"]
pub(crate) struct ExecutableGenerationLaunchReceipt {
    authority: ExactGenerationLaunchAuthority,
    _directory: DirectoryCapability,
    _executable: FileCapability,
    _launch_manifest: FileCapability,
}

impl ExecutableGenerationLaunchReceipt {
    #[must_use]
    pub(in crate::supervised_play) fn into_authority(self) -> ExactGenerationLaunchAuthority {
        self.authority
    }
}

pub(crate) struct ExecutableGenerationStore {
    namespace: PathBuf,
    store_path: PathBuf,
    store_id: String,
    root: Option<DirectoryCapability>,
    owner_marker: Option<OwnerMarker>,
    next_generation: u64,
    next_work_directory: u64,
    active: Option<ImmutableExecutableGeneration>,
    last_good: Option<ImmutableExecutableGeneration>,
    candidate: Option<ImmutableExecutableGeneration>,
    work_directory: Option<PathBuf>,
    cleanup_incomplete: bool,
    closed: bool,
}

impl ExecutableGenerationStore {
    pub(crate) fn create(base_root: &Path) -> Result<Self, GenerationStoreError> {
        Self::create_at(base_root, SystemTime::now())
    }

    fn create_at(base_root: &Path, now: SystemTime) -> Result<Self, GenerationStoreError> {
        scavenge::create_at(base_root, now)
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
        scavenge::scavenge_inactive_at(base_root, now)
    }

    #[cfg(test)]
    pub(crate) fn publish_candidate(
        &mut self,
        artifact_path: &Path,
        manifest: GenerationManifestInput,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        publication::publish_candidate(self, artifact_path, manifest)
    }

    pub(crate) fn publish_candidate_cancellable(
        &mut self,
        artifact_path: &Path,
        manifest: GenerationManifestInput,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        publication::publish_candidate_cancellable(self, artifact_path, manifest, cancel)
    }

    pub(crate) fn activate_candidate(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        retention::activate_candidate(self, generation)
    }

    pub(crate) fn activate_last_good(
        &mut self,
    ) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
        retention::activate_last_good(self)
    }

    pub(crate) fn admit_ready(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<(), GenerationStoreError> {
        retention::admit_ready(self, generation)
    }

    pub(crate) fn finish_active(
        &mut self,
        generation: ExecutableGenerationId,
    ) -> Result<(), GenerationStoreError> {
        retention::finish_active(self, generation)
    }

    #[must_use]
    pub(crate) fn active(&self) -> Option<&ImmutableExecutableGeneration> {
        self.active.as_ref()
    }

    pub(crate) fn begin_work_directory(&mut self) -> Result<PathBuf, GenerationStoreError> {
        retention::begin_work_directory(self)
    }

    pub(crate) fn finish_work_directory(&mut self) -> Result<(), GenerationStoreError> {
        retention::finish_work_directory(self)
    }

    #[must_use]
    pub(crate) fn last_good(&self) -> Option<&ImmutableExecutableGeneration> {
        self.last_good.as_ref()
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn candidate(&self) -> Option<&ImmutableExecutableGeneration> {
        self.candidate.as_ref()
    }

    pub(crate) fn try_close(
        &mut self,
    ) -> Result<GenerationStoreCloseReceipt, GenerationStoreError> {
        retention::try_close(self)
    }

    pub(crate) const fn cleanup_incomplete(&self) -> bool {
        self.cleanup_incomplete
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

    #[cfg(test)]
    pub(in crate::supervised_play) fn set_cleanup_incomplete_for_test(&mut self, incomplete: bool) {
        self.cleanup_incomplete = incomplete;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GenerationStoreCloseReceipt;

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
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn removed(self) -> usize {
        self.removed
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn skipped_live(self) -> usize {
        self.skipped_live
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn skipped_unverified(self) -> usize {
        self.skipped_unverified
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_work_directory_name(value: &str) -> bool {
    value
        .strip_prefix(WORK_DIRECTORY_PREFIX)
        .is_some_and(|sequence| is_lower_hex(sequence, 16))
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

fn component(value: &str) -> Result<RelativeComponent, GenerationStoreError> {
    RelativeComponent::new(value).map_err(|_| GenerationStoreError::InternalNameRejected)
}

#[derive(Debug, Error)]
pub(crate) enum GenerationStoreError {
    #[error("generation publication was cancelled before completion")]
    PublicationCancelled,
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
    #[error("the active product working directory must retire before store close")]
    ActiveWorkDirectoryExists,
    #[error("a product working directory requires an active executable generation")]
    WorkDirectoryRequiresActiveGeneration,
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
    #[error("the published generation name or member identity changed")]
    GenerationBindingChanged,
    #[error("the published generation no longer matches the exact bounded tree")]
    GenerationTreeInvalid,
    #[error("the run store name retry budget was exhausted")]
    RunStoreNameExhausted,
    #[error("the generation sequence was exhausted")]
    GenerationSequenceExhausted,
    #[error("the product working-directory sequence was exhausted")]
    WorkDirectorySequenceExhausted,
    #[error("the product working directory failed exact child validation")]
    WorkDirectoryIdentityInvalid,
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
    #[error("the selected Cargo product identity is invalid: {0}")]
    ProductIdentity(#[source] SupervisedDesktopWireError),
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
