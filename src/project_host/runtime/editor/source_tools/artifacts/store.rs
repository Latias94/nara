//! Capability-relative inventory reads and explicitly confirmed fixed-parent creation.

use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FsError, RelativeComponent, RelativePath,
};

use super::record::{self, Error, Inventory};
use crate::project_host::file_publication::FilePublicationHost;

pub(super) const PATH: &str = ".nara/source-tools/artifacts.json";
pub(super) const PARENT: &str = ".nara/source-tools";
pub(super) const NAME: &str = "artifacts.json";
pub(super) const BYTE_LIMIT: u64 = record::BYTE_LIMIT;

#[derive(Debug)]
pub(super) struct Loaded {
    pub(super) inventory: Inventory,
    pub(super) host: Option<FilePublicationHost>,
}

/// Reads only the inventory. Missing records are empty observations, never ownership authority.
/// Invalid or inaccessible records remain failures; installation members are not opened here.
pub(super) fn load(root: &DirectoryCapability) -> Result<Loaded, Error> {
    let Some(metadata) = optional_directory(root, ".nara")? else {
        return Ok(Loaded {
            inventory: Inventory::default(),
            host: None,
        });
    };
    let Some(parent) = optional_directory(&metadata, "source-tools")? else {
        return Ok(Loaded {
            inventory: Inventory::default(),
            host: None,
        });
    };
    let file = match parent.open_file_exact(&path(NAME)) {
        Ok(file) => file,
        Err(error) if is_missing(&error) => {
            let host = FilePublicationHost::bind_missing(&parent, NAME, BYTE_LIMIT)
                .map_err(|_| Error::Read)?;
            return Ok(Loaded {
                inventory: Inventory::default(),
                host: Some(host),
            });
        }
        Err(error) => return Err(fs_error(error, Error::Read)),
    };
    let bytes = file
        .read_to_end_bounded(BYTE_LIMIT)
        .map_err(|error| fs_error(error, Error::Read))?;
    let inventory = record::decode(&bytes)?;
    let host = FilePublicationHost::from_existing(
        parent,
        component(NAME),
        file,
        ContentDigest::of_bytes(&bytes),
        BYTE_LIMIT,
    );
    Ok(Loaded {
        inventory,
        host: Some(host),
    })
}

/// Creates fixed parents only after confirmation. An existing leaf always refuses, even if equal.
pub(super) fn create_missing_host(
    root: &DirectoryCapability,
) -> Result<FilePublicationHost, Error> {
    if root.options().rights() != CapabilityRights::ReadWrite {
        return Err(Error::ReadOnly);
    }
    let metadata = ensure_directory(root, ".nara")?;
    let parent = ensure_directory(&metadata, "source-tools")?;
    match parent.open_file_exact(&path(NAME)) {
        Ok(_) | Err(FsError::NotRegularFile) => return Err(Error::Changed),
        Err(error) if is_missing(&error) => {}
        Err(error) => return Err(fs_error(error, Error::Write)),
    }
    FilePublicationHost::bind_missing(&parent, NAME, BYTE_LIMIT).map_err(|_| Error::Write)
}

fn optional_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<Option<DirectoryCapability>, Error> {
    match parent.open_directory_exact(&path(name)) {
        Ok(directory) => Ok(Some(directory)),
        Err(error) if is_missing(&error) => Ok(None),
        Err(error) => Err(fs_error(error, Error::Read)),
    }
}

fn ensure_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<DirectoryCapability, Error> {
    let relative = path(name);
    match parent.open_directory_exact(&relative) {
        Ok(directory) => return Ok(directory),
        Err(error) if is_missing(&error) => {}
        Err(error) => return Err(fs_error(error, Error::Write)),
    }
    let created = match parent.create_child_directory(&component(name)) {
        Ok(created) => Some(created),
        Err(FsError::AlreadyExists { .. }) => None,
        Err(error) => return Err(fs_error(error, Error::Write)),
    };
    let opened = parent
        .open_directory_exact(&relative)
        .map_err(|error| fs_error(error, Error::Write))?;
    if let Some(created) = created {
        if !created.same_object(&opened).map_err(|_| Error::Changed)? {
            return Err(Error::Changed);
        }
    }
    Ok(opened)
}

pub(super) fn is_missing(error: &FsError) -> bool {
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

fn path(name: &str) -> RelativePath {
    RelativePath::new(name).expect("the engine-owned artifact inventory path is valid")
}

fn component(name: &str) -> RelativeComponent {
    RelativeComponent::new(name).expect("the engine-owned artifact inventory component is valid")
}

#[cfg(test)]
mod tests;
