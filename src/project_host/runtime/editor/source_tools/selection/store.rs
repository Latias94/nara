//! Bounded code-free selection records and capability-relative filesystem observations.

use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FsError, RelativeComponent, RelativePath,
};
use nara_tooling::{
    EditorSourceToolDesiredSelection, EditorSourceToolInstalledSelection,
    EditorSourceToolSelectionError,
};
use serde::{Deserialize, Serialize};

use crate::project_host::file_publication::FilePublicationHost;

pub(super) const SELECTION_PATH: &str = ".nara/source-tools/selection.json";
pub(super) const BYTE_LIMIT: u64 = 4096;
pub(super) type SelectionError = EditorSourceToolSelectionError;

const METADATA_DIRECTORY: &str = ".nara";
const TOOLS_DIRECTORY: &str = "source-tools";
const RECORD_NAME: &str = "selection.json";
const MAX_DIRECTORY_BYTES: usize = 512;
const MAX_DIRECTORY_COMPONENTS: usize = 16;
const MAX_RECEIPT_BYTES: u64 = 4096;
const MAX_MANIFEST_BYTES: u64 = 65_536;

#[derive(Debug)]
pub(super) struct LoadedSelection {
    pub desired: Option<EditorSourceToolDesiredSelection>,
    pub host: Option<FilePublicationHost>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionRecord {
    format_version: u32,
    installed: InstalledRecord,
    desired_enabled: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledRecord {
    package: String,
    directory: String,
    installation: [u8; 32],
    receipt: DigestRecord,
    manifest: DigestRecord,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestRecord {
    length: u64,
    hash: [u8; 32],
}

/// Reads without creating directories, files or executable authority.
///
/// Only an actual NotFound observation is an unselected slot. Existing corrupt records reject.
/// The publication Host retains the same leaf handle that supplied the decoded bytes, including
/// after a concurrent namespace replacement. Missing bindings rely on strong no-replace at publish.
pub(super) fn load(root: &DirectoryCapability) -> Result<LoadedSelection, SelectionError> {
    let Some(metadata) = open_optional_directory(root, METADATA_DIRECTORY)? else {
        return Ok(LoadedSelection {
            desired: None,
            host: None,
        });
    };
    let Some(parent) = open_optional_directory(&metadata, TOOLS_DIRECTORY)? else {
        return Ok(LoadedSelection {
            desired: None,
            host: None,
        });
    };
    let path = fixed_path(RECORD_NAME);
    let file = match parent.open_file_exact(&path) {
        Ok(file) => file,
        Err(error) if is_missing(&error) => {
            let host = FilePublicationHost::bind_missing(&parent, RECORD_NAME, BYTE_LIMIT)
                .map_err(|_| EditorSourceToolSelectionError::Read)?;
            return Ok(LoadedSelection {
                desired: None,
                host: Some(host),
            });
        }
        Err(error) => return Err(fs_error(error, EditorSourceToolSelectionError::Read)),
    };
    let bytes = file
        .read_to_end_bounded(BYTE_LIMIT)
        .map_err(|error| fs_error(error, EditorSourceToolSelectionError::Read))?;
    let desired = decode(&bytes)?;
    let digest = ContentDigest::of_bytes(&bytes);
    let host = FilePublicationHost::from_existing(
        parent,
        fixed_component(RECORD_NAME),
        file,
        digest,
        BYTE_LIMIT,
    );
    Ok(LoadedSelection {
        desired: Some(desired),
        host: Some(host),
    })
}

/// Creates only the fixed parent directories after an explicitly authorized mutation.
///
/// Never removes directories on failure and never binds an existing leaf for overwrite. The caller
/// must register this Host with HostCustody before publishing; this function does not publish.
pub(super) fn create_missing_host(
    root: &DirectoryCapability,
) -> Result<FilePublicationHost, SelectionError> {
    if root.options().rights() != CapabilityRights::ReadWrite {
        return Err(EditorSourceToolSelectionError::ReadOnly);
    }
    let metadata = ensure_directory(root, METADATA_DIRECTORY)?;
    let parent = ensure_directory(&metadata, TOOLS_DIRECTORY)?;
    match parent.open_file_exact(&fixed_path(RECORD_NAME)) {
        Ok(_) => return Err(EditorSourceToolSelectionError::Changed),
        Err(error) if is_missing(&error) => {}
        Err(FsError::NotRegularFile) => return Err(EditorSourceToolSelectionError::Changed),
        Err(error) => return Err(fs_error(error, EditorSourceToolSelectionError::Write)),
    }
    FilePublicationHost::bind_missing(&parent, RECORD_NAME, BYTE_LIMIT)
        .map_err(|_| EditorSourceToolSelectionError::Write)
}

pub(super) fn encode(
    desired: &EditorSourceToolDesiredSelection,
) -> Result<Vec<u8>, SelectionError> {
    validate(&desired.installed)?;
    let installed = &desired.installed;
    let bytes = serde_json::to_vec(&SelectionRecord {
        format_version: 1,
        installed: InstalledRecord {
            package: installed.package.to_string(),
            directory: installed.directory.to_string(),
            installation: installed.installation,
            receipt: DigestRecord {
                length: installed.receipt.length(),
                hash: *installed.receipt.as_bytes(),
            },
            manifest: DigestRecord {
                length: installed.manifest.length(),
                hash: *installed.manifest.as_bytes(),
            },
        },
        desired_enabled: desired.enabled,
    })
    .map_err(|_| EditorSourceToolSelectionError::InvalidRecord)?;
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(EditorSourceToolSelectionError::TooLarge);
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<EditorSourceToolDesiredSelection, SelectionError> {
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(EditorSourceToolSelectionError::TooLarge);
    }
    let record: SelectionRecord =
        serde_json::from_slice(bytes).map_err(|_| EditorSourceToolSelectionError::InvalidRecord)?;
    if record.format_version != 1 {
        return Err(EditorSourceToolSelectionError::UnsupportedFormat);
    }
    let installed = EditorSourceToolInstalledSelection {
        package: record.installed.package.into(),
        directory: record.installed.directory.into(),
        installation: record.installed.installation,
        receipt: ContentDigest::from_parts(
            record.installed.receipt.length,
            record.installed.receipt.hash,
        ),
        manifest: ContentDigest::from_parts(
            record.installed.manifest.length,
            record.installed.manifest.hash,
        ),
    };
    validate(&installed)?;
    Ok(EditorSourceToolDesiredSelection {
        installed,
        enabled: record.desired_enabled,
    })
}

fn validate(installed: &EditorSourceToolInstalledSelection) -> Result<(), SelectionError> {
    let package = &installed.package;
    if package.is_empty()
        || package.len() > 128
        || !package
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !package.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        || installed.installation == [0; 32]
        || !(1..=MAX_RECEIPT_BYTES).contains(&installed.receipt.length())
        || !(1..=MAX_MANIFEST_BYTES).contains(&installed.manifest.length())
    {
        return Err(EditorSourceToolSelectionError::InvalidRecord);
    }
    if installed.directory.len() > MAX_DIRECTORY_BYTES
        || installed.directory.split('/').count() > MAX_DIRECTORY_COMPONENTS
        || installed
            .directory
            .split('/')
            .any(|component| matches!(component, "" | "." | ".."))
    {
        return Err(EditorSourceToolSelectionError::InvalidPath);
    }
    RelativePath::new(installed.directory.as_ref())
        .map_err(|_| EditorSourceToolSelectionError::InvalidPath)?;
    Ok(())
}

fn open_optional_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<Option<DirectoryCapability>, SelectionError> {
    match parent.open_directory_exact(&fixed_path(name)) {
        Ok(directory) => Ok(Some(directory)),
        Err(error) if is_missing(&error) => Ok(None),
        Err(error) => Err(fs_error(error, EditorSourceToolSelectionError::Read)),
    }
}

fn ensure_directory(
    parent: &DirectoryCapability,
    name: &str,
) -> Result<DirectoryCapability, SelectionError> {
    let path = fixed_path(name);
    match parent.open_directory_exact(&path) {
        Ok(directory) => return Ok(directory),
        Err(error) if is_missing(&error) => {}
        Err(error) => return Err(fs_error(error, EditorSourceToolSelectionError::Write)),
    }
    let created = match parent.create_child_directory(&fixed_component(name)) {
        Ok(created) => Some(created),
        // Another writer may create the directory. Its exact name and kind still need admission.
        Err(FsError::AlreadyExists { .. }) => None,
        Err(error) => return Err(fs_error(error, EditorSourceToolSelectionError::Write)),
    };
    let opened = parent
        .open_directory_exact(&path)
        .map_err(|error| fs_error(error, EditorSourceToolSelectionError::Write))?;
    if created.is_some_and(|created| created.identity() != opened.identity()) {
        return Err(EditorSourceToolSelectionError::Changed);
    }
    Ok(opened)
}

fn is_missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

fn fs_error(error: FsError, fallback: SelectionError) -> SelectionError {
    match error {
        FsError::ByteLimitExceeded { .. } => EditorSourceToolSelectionError::TooLarge,
        FsError::ReadOnlyCapability { .. } => EditorSourceToolSelectionError::ReadOnly,
        FsError::Path(_)
        | FsError::NotDirectory
        | FsError::NotRegularFile
        | FsError::ExactSpellingMismatch { .. }
        | FsError::ReparsePoint { .. }
        | FsError::SymbolicLinkTraversal
        | FsError::CrossVolume
        | FsError::MultipleLinks { .. } => EditorSourceToolSelectionError::InvalidPath,
        FsError::IdentityMismatch { .. }
        | FsError::CapabilityIdentityDrift { .. }
        | FsError::DigestMismatch { .. }
        | FsError::TargetStateMismatch
        | FsError::AlreadyExists { .. } => EditorSourceToolSelectionError::Changed,
        _ => fallback,
    }
}

fn fixed_path(name: &str) -> RelativePath {
    RelativePath::new(name).expect("the engine-owned selection path is valid")
}

fn fixed_component(name: &str) -> RelativeComponent {
    RelativeComponent::new(name).expect("the engine-owned selection component is valid")
}

#[cfg(test)]
mod tests;
