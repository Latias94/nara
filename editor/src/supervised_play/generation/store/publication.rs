use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use nara::fs::{
    ConflictProtection, ContentDigest, DirectoryCapability, DirectoryEnumerationLimits,
    DirectoryPublicationBeginError, DirectoryPublicationOutcome, FileCapability, FileIdentity,
    FileKind, PublicationAtomicity,
};
use nara::project_host::MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES;

use super::super::format::{MAX_LAUNCH_MANIFEST_BYTES, encode_launch_manifest, generation_id};
use super::super::platform::{EXECUTABLE_NAME, artifact_is_executable, open_regular_file};
use super::{
    ExecutableGenerationStore, GenerationManifestInput, GenerationStoreError,
    ImmutableExecutableGeneration, LAUNCH_MANIFEST_NAME, MAX_GENERATION_FILES_PER_DIRECTORY,
    component,
    retention::{evict_candidate, remove_exact_generation_tree},
};

static NEXT_STAGING_NONCE: AtomicU64 = AtomicU64::new(1);

pub(super) struct PublishedGenerationBinding {
    pub(super) store_root: DirectoryCapability,
    pub(super) directory_name: nara::fs::RelativeComponent,
    pub(super) directory: DirectoryCapability,
    pub(super) executable: Arc<FileCapability>,
    pub(super) launch_manifest: Arc<FileCapability>,
}

impl PublishedGenerationBinding {
    pub(super) fn revalidate(
        &self,
        executable_digest: ContentDigest,
        launch_manifest_digest: ContentDigest,
    ) -> Result<(DirectoryCapability, VerifiedGenerationTree), GenerationStoreError> {
        let directory = self
            .store_root
            .open_child_directory(&self.directory_name)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "reopen-generation-directory-for-launch",
                source,
            })?;
        if directory.identity() != self.directory.identity() {
            return Err(GenerationStoreError::GenerationBindingChanged);
        }

        let members = verify_generation_tree(
            &directory,
            Some(GenerationMemberIdentities {
                executable: self.executable.identity(),
                launch_manifest: self.launch_manifest.identity(),
            }),
            executable_digest,
            launch_manifest_digest,
        )?;
        Ok((directory, members))
    }
}

impl Clone for PublishedGenerationBinding {
    fn clone(&self) -> Self {
        Self {
            store_root: self.store_root.share(),
            directory_name: self.directory_name.clone(),
            directory: self.directory.share(),
            executable: Arc::clone(&self.executable),
            launch_manifest: Arc::clone(&self.launch_manifest),
        }
    }
}

#[cfg(test)]
pub(super) fn publish_candidate(
    store: &mut ExecutableGenerationStore,
    artifact_path: &Path,
    manifest: GenerationManifestInput,
) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
    publish_candidate_cancellable(store, artifact_path, manifest, &AtomicBool::new(false))
}

pub(super) fn publish_candidate_cancellable(
    store: &mut ExecutableGenerationStore,
    artifact_path: &Path,
    manifest: GenerationManifestInput,
    cancel: &AtomicBool,
) -> Result<ImmutableExecutableGeneration, GenerationStoreError> {
    check_cancel(cancel)?;
    store.ensure_operational()?;
    if !artifact_path.is_absolute() {
        return Err(GenerationStoreError::ArtifactPathNotAbsolute);
    }
    evict_candidate(store)?;

    let source = open_artifact(artifact_path)?;
    let executable_digest = source
        .digest(MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "digest-source-executable",
            source,
        })?;
    let sequence = store.next_generation;
    check_cancel(cancel)?;
    let next_generation = sequence
        .checked_add(1)
        .ok_or(GenerationStoreError::GenerationSequenceExhausted)?;
    let generation_id = generation_id(&store.store_id, sequence, executable_digest);
    let product_identity = manifest.product_identity()?;
    let manifest_bytes = encode_launch_manifest(generation_id, executable_digest, &manifest)?;
    let manifest_digest = ContentDigest::of_bytes(&manifest_bytes);
    let source_receipt = Arc::new(manifest.into_source_receipt());
    let destination_name = format!("generation-{}", generation_id.hex());
    let staging_nonce = NEXT_STAGING_NONCE.fetch_add(1, Ordering::Relaxed);
    let staging_name = format!(
        ".generation-{}.stage-{staging_nonce:016x}",
        generation_id.hex()
    );
    let destination_component = component(&destination_name)?;
    let staging_component = component(&staging_name)?;
    let root = store
        .root
        .as_ref()
        .ok_or(GenerationStoreError::StoreClosed)?;
    let transaction =
        match root.begin_directory_publication(&destination_component, &staging_component) {
            Ok(transaction) => transaction,
            Err(DirectoryPublicationBeginError::Orphaned(orphan)) => {
                drop(orphan);
                if !remove_exact_generation_tree(&store.store_path, &staging_name) {
                    store.cleanup_incomplete = true;
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
        cancel,
    );
    drop(staging);
    if let Err(error) = populate_result {
        let outcome = transaction.abort();
        drop(outcome);
        if !remove_exact_generation_tree(&store.store_path, &staging_name) {
            store.cleanup_incomplete = true;
            return Err(GenerationStoreError::StagingCleanupIncomplete);
        }
        return Err(error);
    }

    let published_directory_identity = match transaction.publish() {
        DirectoryPublicationOutcome::Published(receipt)
            if receipt.publication_atomicity() == PublicationAtomicity::AtomicNameSwitch
                && receipt.conflict_protection() == ConflictProtection::StrongCompareAndSwap
                && receipt.parent_identity() == root.identity()
                && receipt.candidate_identity() == receipt.published_identity()
                && receipt.destination() == &destination_component =>
        {
            receipt.published_identity()
        }
        DirectoryPublicationOutcome::Published(_) => {
            if !remove_exact_generation_tree(&store.store_path, &destination_name) {
                store.cleanup_incomplete = true;
                return Err(GenerationStoreError::PublishedCleanupIncomplete);
            }
            return Err(GenerationStoreError::PublicationProofInsufficient);
        }
        outcome => {
            drop(outcome);
            if !remove_exact_generation_tree(&store.store_path, &staging_name) {
                store.cleanup_incomplete = true;
                return Err(GenerationStoreError::StagingCleanupIncomplete);
            }
            return Err(GenerationStoreError::PublicationRejected);
        }
    };

    let directory = store.store_path.join(&destination_name);
    let executable = directory.join(EXECUTABLE_NAME);
    let launch_manifest = directory.join(LAUNCH_MANIFEST_NAME);
    let verification = root
        .open_child_directory(&destination_component)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "open-published-generation-directory",
            source,
        })
        .and_then(|published_directory| {
            if published_directory.identity() != published_directory_identity {
                return Err(GenerationStoreError::GenerationBindingChanged);
            }
            let members = verify_generation_tree(
                &published_directory,
                None,
                executable_digest,
                manifest_digest,
            )?;
            Ok((published_directory, members))
        });
    let (directory_authority, members) = match verification {
        Ok(verified) => verified,
        Err(error) => {
            if !remove_exact_generation_tree(&store.store_path, &destination_name) {
                store.cleanup_incomplete = true;
                return Err(GenerationStoreError::PublishedCleanupIncomplete);
            }
            return Err(error);
        }
    };

    let generation = ImmutableExecutableGeneration {
        id: generation_id,
        sequence,
        product_identity,
        binding: PublishedGenerationBinding {
            store_root: root.share(),
            directory_name: destination_component,
            directory: directory_authority,
            executable: Arc::new(members.executable),
            launch_manifest: Arc::new(members.launch_manifest),
        },
        directory,
        executable,
        launch_manifest,
        executable_digest,
        launch_manifest_digest: manifest_digest,
        source_receipt,
    };
    store.next_generation = next_generation;
    store.candidate = Some(generation.clone());
    Ok(generation)
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
    FileCapability::from_host_handle(file, nara::fs::TrustMode::TrustedLocal, 0).map_err(|source| {
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
    cancel: &AtomicBool,
) -> Result<(), GenerationStoreError> {
    check_cancel(cancel)?;
    let executable_name = component(EXECUTABLE_NAME)?;
    let mut executable = staging
        .create_child_executable_file(&executable_name)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "create-generation-executable",
            source,
        })?;
    copy_source_executable(source, &mut executable, expected_executable_digest, cancel)?;
    check_cancel(cancel)?;
    executable
        .sync()
        .map_err(|source| GenerationStoreError::Capability {
            operation: "sync-generation-executable",
            source,
        })?;
    executable
        .verify_digest(
            expected_executable_digest,
            MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES,
        )
        .map_err(|source| GenerationStoreError::Capability {
            operation: "verify-generation-executable",
            source,
        })?;
    let source_after = source
        .digest(MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "redigest-source-executable",
            source,
        })?;
    if source_after != expected_executable_digest {
        return Err(GenerationStoreError::ArtifactChangedDuringCopy);
    }
    check_cancel(cancel)?;

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
    check_cancel(cancel)?;
    Ok(())
}

fn copy_source_executable(
    source: &FileCapability,
    destination: &mut FileCapability,
    expected: ContentDigest,
    cancel: &AtomicBool,
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
        check_cancel(cancel)?;
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
        if copied > expected.length() || copied > MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES {
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

fn check_cancel(cancel: &AtomicBool) -> Result<(), GenerationStoreError> {
    if cancel.load(Ordering::Acquire) {
        Err(GenerationStoreError::PublicationCancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct GenerationMemberIdentities {
    executable: FileIdentity,
    launch_manifest: FileIdentity,
}

pub(super) struct VerifiedGenerationTree {
    pub(super) executable: FileCapability,
    pub(super) launch_manifest: FileCapability,
}

fn verify_generation_tree(
    directory: &DirectoryCapability,
    expected_identities: Option<GenerationMemberIdentities>,
    expected_executable_digest: ContentDigest,
    expected_manifest_digest: ContentDigest,
) -> Result<VerifiedGenerationTree, GenerationStoreError> {
    let mut enumeration = directory
        .begin_child_enumeration(DirectoryEnumerationLimits::new(
            MAX_GENERATION_FILES_PER_DIRECTORY,
            64,
            0,
        ))
        .map_err(|source| GenerationStoreError::Capability {
            operation: "begin-generation-tree-enumeration",
            source,
        })?;
    let mut observed_executable = None;
    let mut observed_manifest = None;
    let mut enumeration_complete = false;
    for _ in 0..=MAX_GENERATION_FILES_PER_DIRECTORY {
        let batch = enumeration
            .next_batch(MAX_GENERATION_FILES_PER_DIRECTORY)
            .map_err(|source| GenerationStoreError::Capability {
                operation: "enumerate-generation-tree",
                source,
            })?;
        if !batch.rejected_entry_facts().is_empty() {
            return Err(GenerationStoreError::GenerationTreeInvalid);
        }
        for entry in batch.entries() {
            if entry.kind() != FileKind::Regular {
                return Err(GenerationStoreError::GenerationTreeInvalid);
            }
            let Some(identity) = entry.identity() else {
                return Err(GenerationStoreError::GenerationTreeInvalid);
            };
            let slot = match entry.name().to_str() {
                Some(EXECUTABLE_NAME) => &mut observed_executable,
                Some(LAUNCH_MANIFEST_NAME) => &mut observed_manifest,
                _ => return Err(GenerationStoreError::GenerationTreeInvalid),
            };
            if slot.replace(identity).is_some() {
                return Err(GenerationStoreError::GenerationTreeInvalid);
            }
        }
        if batch.is_complete() {
            enumeration_complete = true;
            break;
        }
    }
    if !enumeration_complete {
        return Err(GenerationStoreError::GenerationTreeInvalid);
    }

    let observed = GenerationMemberIdentities {
        executable: observed_executable.ok_or(GenerationStoreError::GenerationTreeInvalid)?,
        launch_manifest: observed_manifest.ok_or(GenerationStoreError::GenerationTreeInvalid)?,
    };
    if expected_identities.is_some_and(|expected| {
        expected.executable != observed.executable
            || expected.launch_manifest != observed.launch_manifest
    }) {
        return Err(GenerationStoreError::GenerationBindingChanged);
    }

    let executable_name = component(EXECUTABLE_NAME)?;
    let executable = directory
        .open_child_file(&executable_name)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "open-generation-executable",
            source,
        })?;
    verify_generation_member_binding(
        directory,
        &executable,
        observed.executable,
        expected_executable_digest,
        MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES,
        "verify-generation-executable",
    )?;

    let manifest_name = component(LAUNCH_MANIFEST_NAME)?;
    let launch_manifest = directory
        .open_child_file(&manifest_name)
        .map_err(|source| GenerationStoreError::Capability {
            operation: "open-generation-launch-manifest",
            source,
        })?;
    verify_generation_member_binding(
        directory,
        &launch_manifest,
        observed.launch_manifest,
        expected_manifest_digest,
        MAX_LAUNCH_MANIFEST_BYTES as u64,
        "verify-generation-launch-manifest",
    )?;

    Ok(VerifiedGenerationTree {
        executable,
        launch_manifest,
    })
}

fn verify_generation_member_binding(
    directory: &DirectoryCapability,
    file: &FileCapability,
    observed_identity: FileIdentity,
    expected_digest: ContentDigest,
    limit: u64,
    operation: &'static str,
) -> Result<(), GenerationStoreError> {
    if file.identity() != observed_identity
        || file.parent_identity() != Some(directory.identity())
        || file.link_count() != 1
    {
        return Err(GenerationStoreError::GenerationBindingChanged);
    }
    file.verify_digest(expected_digest, limit)
        .map_err(|source| GenerationStoreError::Capability { operation, source })
}
