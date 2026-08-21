use std::{
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

use nara::fs::ContentDigest;
use thiserror::Error;

use super::{DesktopBuildRequest, MAX_LOCKFILE_BYTES, MAX_MANIFEST_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DesktopBuildInputRevision {
    manifest: ContentDigest,
    lockfile: ContentDigest,
}

impl DesktopBuildInputRevision {
    pub(crate) const fn new(manifest: ContentDigest, lockfile: ContentDigest) -> Self {
        Self { manifest, lockfile }
    }

    pub(crate) const fn manifest(self) -> ContentDigest {
        self.manifest
    }

    pub(crate) const fn lockfile(self) -> ContentDigest {
        self.lockfile
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopBuildInputMember {
    Manifest,
    Lockfile,
}

impl std::fmt::Display for DesktopBuildInputMember {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Manifest => "Cargo manifest",
            Self::Lockfile => "Cargo lockfile",
        })
    }
}

#[derive(Debug, Error)]
pub(crate) enum DesktopBuildInputError {
    #[error("the {member} is not a regular file")]
    NotRegular { member: DesktopBuildInputMember },
    #[error("the {member} exceeded its bounded size")]
    LimitExceeded { member: DesktopBuildInputMember },
    #[error("the {member} could not be read")]
    Read {
        member: DesktopBuildInputMember,
        #[source]
        source: io::Error,
    },
}

impl DesktopBuildInputError {
    pub(crate) const fn member(&self) -> DesktopBuildInputMember {
        match self {
            Self::NotRegular { member }
            | Self::LimitExceeded { member }
            | Self::Read { member, .. } => *member,
        }
    }
}

pub(crate) fn capture_input_revision(
    request: &DesktopBuildRequest,
) -> Result<DesktopBuildInputRevision, DesktopBuildInputError> {
    capture_input_revision_at(&request.manifest_path, &request.lockfile_path)
}

pub(crate) fn capture_input_revision_at(
    manifest_path: &Path,
    lockfile_path: &Path,
) -> Result<DesktopBuildInputRevision, DesktopBuildInputError> {
    Ok(DesktopBuildInputRevision {
        manifest: digest_input_member(
            manifest_path,
            DesktopBuildInputMember::Manifest,
            MAX_MANIFEST_BYTES,
        )?,
        lockfile: digest_input_member(
            lockfile_path,
            DesktopBuildInputMember::Lockfile,
            MAX_LOCKFILE_BYTES,
        )?,
    })
}

fn digest_input_member(
    path: &Path,
    member: DesktopBuildInputMember,
    limit: u64,
) -> Result<ContentDigest, DesktopBuildInputError> {
    let metadata =
        fs::metadata(path).map_err(|source| DesktopBuildInputError::Read { member, source })?;
    if !metadata.is_file() {
        return Err(DesktopBuildInputError::NotRegular { member });
    }
    if metadata.len() > limit {
        return Err(DesktopBuildInputError::LimitExceeded { member });
    }
    let file =
        File::open(path).map_err(|source| DesktopBuildInputError::Read { member, source })?;
    let allocation_limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let initial_capacity = usize::try_from(metadata.len())
        .unwrap_or(allocation_limit)
        .min(allocation_limit);
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(initial_capacity)
        .map_err(|_| DesktopBuildInputError::Read {
            member,
            source: io::Error::new(
                io::ErrorKind::OutOfMemory,
                "bounded input allocation failed",
            ),
        })?;
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| DesktopBuildInputError::Read { member, source })?;
    if bytes.len() > allocation_limit {
        return Err(DesktopBuildInputError::LimitExceeded { member });
    }
    Ok(ContentDigest::of_bytes(&bytes))
}

pub(crate) fn revision_mismatch(
    expected: DesktopBuildInputRevision,
    current: DesktopBuildInputRevision,
) -> Option<DesktopBuildInputMember> {
    if expected.manifest != current.manifest {
        Some(DesktopBuildInputMember::Manifest)
    } else if expected.lockfile != current.lockfile {
        Some(DesktopBuildInputMember::Lockfile)
    } else {
        None
    }
}
