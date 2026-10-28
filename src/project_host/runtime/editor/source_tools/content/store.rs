//! Capability-relative observations and explicit parent creation for the private content record.

use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FsError, RelativeComponent, RelativePath,
};

use super::record::{self, Error, Ledger};
use crate::project_host::file_publication::FilePublicationHost;

pub(super) const PATH: &str = ".nara/source-tools/content.json";
pub(super) const BYTE_LIMIT: u64 = record::BYTE_LIMIT;
const METADATA_DIRECTORY: &str = ".nara";
const TOOLS_DIRECTORY: &str = "source-tools";
const RECORD_NAME: &str = "content.json";

#[derive(Debug)]
pub(super) struct Loaded {
    pub(super) ledger: Ledger,
    pub(super) host: Option<FilePublicationHost>,
}

/// Observes only the ledger; installed payloads may be missing, modified or adopted.
///
/// Missing records contain no management or deletion authority. A present record retains the
/// actual decoded leaf through Host admission; corrupt or inaccessible records never become empty.
pub(super) fn load(root: &DirectoryCapability) -> Result<Loaded, Error> {
    let Some(metadata) = open_optional_directory(root, METADATA_DIRECTORY)? else {
        return Ok(Loaded {
            ledger: Ledger::default(),
            host: None,
        });
    };
    let Some(parent) = open_optional_directory(&metadata, TOOLS_DIRECTORY)? else {
        return Ok(Loaded {
            ledger: Ledger::default(),
            host: None,
        });
    };
    let file = match parent.open_file_exact(&fixed_path(RECORD_NAME)) {
        Ok(file) => file,
        Err(error) if is_missing(&error) => {
            let host = FilePublicationHost::bind_missing(&parent, RECORD_NAME, BYTE_LIMIT)
                .map_err(|_| Error::Read)?;
            return Ok(Loaded {
                ledger: Ledger::default(),
                host: Some(host),
            });
        }
        Err(error) => return Err(fs_error(error, Error::Read)),
    };
    let bytes = file
        .read_to_end_bounded(BYTE_LIMIT)
        .map_err(|error| fs_error(error, Error::Read))?;
    let ledger = record::decode(&bytes)?;
    let host = FilePublicationHost::from_existing(
        parent,
        fixed_component(RECORD_NAME),
        file,
        ContentDigest::of_bytes(&bytes),
        BYTE_LIMIT,
    );
    Ok(Loaded {
        ledger,
        host: Some(host),
    })
}

/// Creates only the fixed parents for an explicitly admitted mutation; never publishes or deletes.
///
/// Existing leaves reject even when their bytes match. The caller retains this Host through the
/// existing HostCustody and Tasks owners before any publication.
pub(super) fn create_missing_host(
    root: &DirectoryCapability,
) -> Result<FilePublicationHost, Error> {
    if root.options().rights() != CapabilityRights::ReadWrite {
        return Err(Error::ReadOnly);
    }
    let metadata = ensure_directory(root, METADATA_DIRECTORY)?;
    let parent = ensure_directory(&metadata, TOOLS_DIRECTORY)?;
    match parent.open_file_exact(&fixed_path(RECORD_NAME)) {
        Ok(_) => return Err(Error::Changed),
        Err(error) if is_missing(&error) => {}
        Err(FsError::NotRegularFile) => return Err(Error::Changed),
        Err(error) => return Err(fs_error(error, Error::Write)),
    }
    FilePublicationHost::bind_missing(&parent, RECORD_NAME, BYTE_LIMIT).map_err(|_| Error::Write)
}

fn open_optional_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<Option<DirectoryCapability>, Error> {
    match parent.open_directory_exact(&fixed_path(name)) {
        Ok(directory) => Ok(Some(directory)),
        Err(error) if is_missing(&error) => Ok(None),
        Err(error) => Err(fs_error(error, Error::Read)),
    }
}

fn ensure_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<DirectoryCapability, Error> {
    let path = fixed_path(name);
    match parent.open_directory_exact(&path) {
        Ok(directory) => return Ok(directory),
        Err(error) if is_missing(&error) => {}
        Err(error) => return Err(fs_error(error, Error::Write)),
    }
    let created = match parent.create_child_directory(&fixed_component(name)) {
        Ok(created) => Some(created),
        Err(FsError::AlreadyExists { .. }) => None,
        Err(error) => return Err(fs_error(error, Error::Write)),
    };
    let opened = parent
        .open_directory_exact(&path)
        .map_err(|error| fs_error(error, Error::Write))?;
    if created.is_some_and(|created| created.identity() != opened.identity()) {
        return Err(Error::Changed);
    }
    Ok(opened)
}

fn is_missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

fn fs_error(error: FsError, fallback: Error) -> Error {
    match error {
        FsError::ByteLimitExceeded { .. } => Error::TooLarge,
        FsError::ReadOnlyCapability { .. } => Error::ReadOnly,
        FsError::Path(_)
        | FsError::NotDirectory
        | FsError::NotRegularFile
        | FsError::ExactSpellingMismatch { .. }
        | FsError::ReparsePoint { .. }
        | FsError::SymbolicLinkTraversal
        | FsError::CrossVolume
        | FsError::MultipleLinks { .. } => Error::InvalidPath,
        FsError::IdentityMismatch { .. }
        | FsError::CapabilityIdentityDrift { .. }
        | FsError::DigestMismatch { .. }
        | FsError::TargetStateMismatch
        | FsError::AlreadyExists { .. } => Error::Changed,
        _ => fallback,
    }
}

fn fixed_path(name: &str) -> RelativePath {
    RelativePath::new(name).expect("the engine-owned content path is valid")
}

fn fixed_component(name: &str) -> RelativeComponent {
    RelativeComponent::new(name).expect("the engine-owned content component is valid")
}

#[cfg(test)]
mod tests;
