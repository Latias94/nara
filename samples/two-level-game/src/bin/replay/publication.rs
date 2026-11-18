use std::{
    fs::File,
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use nara::fs::{
    CapabilityRights, ConflictProtection, ContentDigest, DirectoryCapability, DurabilityProgress,
    ExpectedTarget, FsError, FsOperation, HostCapabilityOptions, PublicationAtomicity,
    RelativeComponent, StageStatus, TemporaryFile, TemporaryFileFailure, TrustMode,
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub(super) struct Publication {
    status: &'static str,
    durability: DurabilityProgress,
}

impl Publication {
    pub(super) fn not_published() -> Self {
        Self {
            status: "not_published",
            durability: DurabilityProgress::NONE,
        }
    }

    pub(super) fn result(self) -> Value {
        let stage = |status| match status {
            StageStatus::Achieved => "achieved",
            StageStatus::Unsupported => "unsupported",
            StageStatus::Unknown => "unknown",
        };
        json!({
            "status": self.status,
            "data_synced": stage(self.durability.data_synced()),
            "file_metadata_synced": stage(self.durability.file_metadata_synced()),
            "name_published": stage(self.durability.name_published()),
            "parent_directory_synced": stage(self.durability.parent_directory_synced()),
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("replay output publication failed: {source}")]
pub(super) struct PublicationFailure {
    #[source]
    source: TemporaryFileFailure,
    publication: Publication,
}

impl PublicationFailure {
    fn new(error: impl Into<TemporaryFileFailure>, publication: Publication) -> Self {
        Self {
            source: error.into(),
            publication,
        }
    }

    fn candidate(error: FsError, candidate: TemporaryFile, publication: Publication) -> Self {
        Self::new(
            TemporaryFileFailure::with_cleanup(error, candidate.into_cleanup()),
            publication,
        )
    }

    pub(super) fn publication(&self) -> Publication {
        self.publication
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        self.source.cleanup().is_some()
    }

    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.source.retry_cleanup()
    }

    /// Preserves pending native cleanup authority through the CLI's terminal result and exit.
    pub(super) fn retain_cleanup_until_exit(self) {
        if let Some(cleanup) = self.source.into_parts().1 {
            std::mem::forget(cleanup);
        }
    }
}

pub(super) fn publish(path: &Path, bytes: &[u8]) -> Result<Publication, PublicationFailure> {
    publish_inner(
        path,
        bytes,
        #[cfg(test)]
        Faults::default(),
    )
}

fn publish_inner(
    path: &Path,
    bytes: &[u8],
    #[cfg(test)] mut faults: Faults,
) -> Result<Publication, PublicationFailure> {
    let mut publication = Publication::not_published();
    let invalid_path = || {
        PublicationFailure::new(
            FsError::Unproven {
                operation: FsOperation::CreateTemporary,
                proof: "replay output requires an absolute file path with an existing parent",
            },
            publication,
        )
    };
    if !path.is_absolute() {
        return Err(invalid_path());
    }
    let parent_path = path.parent().ok_or_else(invalid_path)?;
    let target = RelativeComponent::new(path.file_name().ok_or_else(invalid_path)?)
        .map_err(|error| PublicationFailure::new(FsError::from(error), publication))?;
    let parent =
        open_parent(parent_path).map_err(|error| PublicationFailure::new(error, publication))?;
    parent
        .preflight_replace(ExpectedTarget::Missing)
        .map_err(|error| PublicationFailure::new(error, publication))?;
    match parent.open_child_file(&target) {
        Ok(_) => {
            return Err(PublicationFailure::new(
                FsError::TargetStateMismatch,
                publication,
            ));
        }
        Err(FsError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(PublicationFailure::new(error, publication)),
    }
    let (mut candidate, name) = create_candidate(&parent, &target)
        .map_err(|error| PublicationFailure::new(error, publication))?;
    #[cfg(test)]
    if let Some(limit) = faults.fail_write_after {
        let error = candidate
            .write_all(&bytes[..limit.min(bytes.len())])
            .err()
            .unwrap_or_else(|| io::Error::other("injected replay write failure"));
        return Err(PublicationFailure::candidate(
            FsError::Io {
                operation: FsOperation::Write,
                source: error,
            },
            candidate,
            publication,
        ));
    }
    if let Err(source) = candidate.write_all(bytes).and_then(|()| candidate.flush()) {
        return Err(PublicationFailure::candidate(
            FsError::Io {
                operation: FsOperation::Write,
                source,
            },
            candidate,
            publication,
        ));
    }
    #[cfg(test)]
    if let Err(error) = faults.checkpoint(Checkpoint::Written) {
        return Err(PublicationFailure::candidate(error, candidate, publication));
    }
    let sync = match candidate.sync() {
        Ok(sync) => sync,
        Err(error) => return Err(PublicationFailure::candidate(error, candidate, publication)),
    };
    publication.durability = sync.progress();
    if sync.progress().data_synced() != StageStatus::Achieved
        || sync.progress().file_metadata_synced() != StageStatus::Achieved
    {
        return Err(PublicationFailure::candidate(
            FsError::Unproven {
                operation: FsOperation::SyncFile,
                proof: "complete replay data and file metadata must be synced before publication",
            },
            candidate,
            publication,
        ));
    }
    let retained = match parent.open_child_file(&name) {
        Ok(file) => file,
        Err(error) => return Err(PublicationFailure::candidate(error, candidate, publication)),
    };
    if retained.identity() != candidate.identity() {
        return Err(PublicationFailure::candidate(
            FsError::IdentityMismatch {
                expected: candidate.identity(),
                actual: retained.identity(),
            },
            candidate,
            publication,
        ));
    }
    #[cfg(test)]
    if let Err(error) = faults.checkpoint(Checkpoint::Synced) {
        return Err(PublicationFailure::candidate(error, candidate, publication));
    }
    let receipt = parent
        .replace_temp(candidate, &target, ExpectedTarget::Missing)
        .map_err(|error| PublicationFailure::new(error, publication))?;
    publication.status = "published_uncertain";
    publication.durability = receipt.durability();
    #[cfg(test)]
    faults
        .checkpoint(Checkpoint::Published)
        .map_err(|error| PublicationFailure::new(error, publication))?;
    let digest = ContentDigest::of_bytes(bytes);
    if receipt.previous_identity().is_some()
        || receipt.publication_atomicity() != PublicationAtomicity::AtomicNameSwitch
        || receipt.conflict_protection() != ConflictProtection::StrongCompareAndSwap
        || receipt.candidate_identity() != retained.identity()
        || receipt.published_identity() != Some(retained.identity())
        || receipt.verified_content_digest() != Some(digest)
        || receipt.durability().data_synced() != StageStatus::Achieved
        || receipt.durability().file_metadata_synced() != StageStatus::Achieved
        || receipt.durability().name_published() != StageStatus::Achieved
    {
        return Err(PublicationFailure::new(
            FsError::Unproven {
                operation: FsOperation::Replace,
                proof: "the complete synced replay publication could not be verified",
            },
            publication,
        ));
    }
    let published = parent
        .open_child_file(&target)
        .map_err(|error| PublicationFailure::new(error, publication))?;
    if published.identity() != retained.identity() {
        return Err(PublicationFailure::new(
            FsError::IdentityMismatch {
                expected: retained.identity(),
                actual: published.identity(),
            },
            publication,
        ));
    }
    published
        .verify_digest(digest, bytes.len() as u64)
        .map_err(|error| PublicationFailure::new(error, publication))?;
    publication.durability = DurabilityProgress::new(
        receipt.durability().data_synced(),
        receipt.durability().file_metadata_synced(),
        receipt.durability().name_published(),
        StageStatus::Unknown,
    );
    let directory_sync = || {
        parent
            .sync()
            .map(|sync| sync.progress().parent_directory_synced())
    };
    #[cfg(test)]
    let synced = faults.parent_sync.unwrap_or_else(directory_sync);
    #[cfg(not(test))]
    let synced = directory_sync();
    let parent_synced = match synced {
        Ok(status) => status,
        Err(FsError::Unsupported { .. }) => StageStatus::Unsupported,
        Err(FsError::Unproven { .. }) => StageStatus::Unknown,
        Err(error) => return Err(PublicationFailure::new(error, publication)),
    };
    publication.durability = DurabilityProgress::new(
        receipt.durability().data_synced(),
        receipt.durability().file_metadata_synced(),
        receipt.durability().name_published(),
        parent_synced,
    );
    publication.status = "published";
    Ok(publication)
}

fn open_parent(path: &Path) -> Result<DirectoryCapability, FsError> {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        File::options()
            .read(true)
            .write(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
    };
    #[cfg(not(windows))]
    let file = File::open(path);
    DirectoryCapability::from_host_handle(
        file.map_err(|source| FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        })?,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
}

fn create_candidate(
    parent: &DirectoryCapability,
    target: &RelativeComponent,
) -> Result<(TemporaryFile, RelativeComponent), TemporaryFileFailure> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    for _ in 0..16 {
        let name = RelativeComponent::new(format!(
            ".two-level-replay-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .expect("static replay temporary name");
        if target.to_str().is_some_and(|target| {
            target.eq_ignore_ascii_case(name.to_str().expect("static replay temporary name"))
        }) {
            continue;
        }
        match parent.create_temp(&name) {
            Err(failure)
                if failure.cleanup().is_none()
                    && matches!(failure.error(), FsError::AlreadyExists { .. }) =>
            {
                continue;
            }
            result => return result.map(|candidate| (candidate, name)),
        }
    }
    Err(FsError::AlreadyExists {
        operation: FsOperation::CreateTemporary,
    }
    .into())
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Checkpoint {
    Written,
    Synced,
    Published,
}

#[cfg(test)]
#[derive(Default)]
struct Faults {
    fail_write_after: Option<usize>,
    checkpoint: Option<Box<dyn FnMut(Checkpoint) -> Result<(), FsError>>>,
    parent_sync: Option<Result<StageStatus, FsError>>,
}

#[cfg(test)]
impl Faults {
    fn checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), FsError> {
        self.checkpoint
            .as_mut()
            .map_or(Ok(()), |hook| hook(checkpoint))
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
#[path = "publication/tests.rs"]
mod tests;
