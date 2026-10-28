//! Bounded code-free selection records and capability-relative filesystem observations.

use std::{borrow::Cow, io::Write};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape};
use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FsError, RelativeComponent, RelativePath,
};
use nara_tooling::{
    EditorSourceToolDesiredSelection as Desired, EditorSourceToolInstalledEntry as Entry,
    EditorSourceToolInstalledSelection, EditorSourceToolRequirementBinding as Binding,
    EditorSourceToolSelectionError as Error,
};
use serde::{Deserialize, Serialize};

use super::installed_graph::{
    Graph, MAX_DIRECTORY_BYTES, MAX_EDGES, MAX_ENTRIES, MAX_REQUIREMENTS,
};
use crate::project_host::file_publication::FilePublicationHost;

pub(super) const SELECTION_PATH: &str = ".nara/source-tools/selection.json";
pub(super) const BYTE_LIMIT: u64 = 65_536;

const METADATA_DIRECTORY: &str = ".nara";
const TOOLS_DIRECTORY: &str = "source-tools";
const RECORD_NAME: &str = "selection.json";

#[derive(Debug)]
pub(super) struct LoadedSelection {
    pub graph: Graph,
    pub host: Option<FilePublicationHost>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionRecord<'a> {
    format_version: u32,
    #[serde(borrow)]
    installed: Vec<InstalledRecord<'a>>,
    desired: Option<DesiredRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledRecord<'a> {
    #[serde(borrow)]
    package: Cow<'a, str>,
    #[serde(borrow)]
    directory: Cow<'a, str>,
    installation: [u8; 32],
    receipt: DigestRecord,
    manifest: DigestRecord,
    #[serde(borrow)]
    requires: Vec<RequirementRecord<'a>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequirementRecord<'a> {
    #[serde(borrow)]
    package: Cow<'a, str>,
    installation: [u8; 32],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DesiredRecord {
    installation: [u8; 32],
    enabled: bool,
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
pub(super) fn load(root: &DirectoryCapability) -> Result<LoadedSelection, Error> {
    let Some(metadata) = open_optional_directory(root, METADATA_DIRECTORY)? else {
        return Ok(LoadedSelection {
            graph: Graph::default(),
            host: None,
        });
    };
    let Some(parent) = open_optional_directory(&metadata, TOOLS_DIRECTORY)? else {
        return Ok(LoadedSelection {
            graph: Graph::default(),
            host: None,
        });
    };
    let path = fixed_path(RECORD_NAME);
    let file = match parent.open_file_exact(&path) {
        Ok(file) => file,
        Err(error) if is_missing(&error) => {
            let host = FilePublicationHost::bind_missing(&parent, RECORD_NAME, BYTE_LIMIT)
                .map_err(|_| Error::Read)?;
            return Ok(LoadedSelection {
                graph: Graph::default(),
                host: Some(host),
            });
        }
        Err(error) => return Err(fs_error(error, Error::Read)),
    };
    let bytes = file
        .read_to_end_bounded(BYTE_LIMIT)
        .map_err(|error| fs_error(error, Error::Read))?;
    let graph = decode(&bytes)?;
    let digest = ContentDigest::of_bytes(&bytes);
    let host = FilePublicationHost::from_existing(
        parent,
        fixed_component(RECORD_NAME),
        file,
        digest,
        BYTE_LIMIT,
    );
    Ok(LoadedSelection {
        graph,
        host: Some(host),
    })
}

/// Creates only the fixed parent directories after an explicitly authorized mutation.
///
/// Never removes directories on failure and never binds an existing leaf for overwrite. The caller
/// must register this Host with HostCustody before publishing; this function does not publish.
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

pub(super) fn encode(graph: &Graph) -> Result<Vec<u8>, Error> {
    graph.validate()?;
    let mut installed: Vec<_> = graph
        .entries
        .iter()
        .map(|entry| {
            let installed = &entry.installed;
            let mut requires: Vec<_> = entry
                .requirements
                .iter()
                .map(|binding| RequirementRecord {
                    package: Cow::Borrowed(binding.package.as_ref()),
                    installation: binding.installation,
                })
                .collect();
            requires.sort_unstable_by(|left, right| left.package.cmp(&right.package));
            InstalledRecord {
                package: Cow::Borrowed(installed.package.as_ref()),
                directory: Cow::Borrowed(installed.directory.as_ref()),
                installation: installed.installation,
                receipt: DigestRecord {
                    length: installed.receipt.length(),
                    hash: *installed.receipt.as_bytes(),
                },
                manifest: DigestRecord {
                    length: installed.manifest.length(),
                    hash: *installed.manifest.as_bytes(),
                },
                requires,
            }
        })
        .collect();
    installed.sort_unstable_by_key(|entry| entry.installation);
    let record = SelectionRecord {
        format_version: 1,
        installed,
        desired: graph.desired.as_ref().map(|desired| DesiredRecord {
            installation: desired.installed.installation,
            enabled: desired.enabled,
        }),
    };
    let mut output = RecordBytes {
        bytes: Vec::new(),
        oversized: false,
    };
    if serde_json::to_writer(&mut output, &record).is_err() {
        return Err(if output.oversized {
            Error::TooLarge
        } else {
            Error::InvalidRecord
        });
    }
    Ok(output.bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Graph, Error> {
    if bytes.len() as u64 > BYTE_LIMIT {
        return Err(Error::TooLarge);
    }
    // Every valid array has at most 32 items, including fixed-width hashes. These limits run
    // before DTO vectors and owned strings, and duplicate keys reject before domain allocation.
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(8).expect("nonzero selection depth"),
        ItemLimit::new(10_000).expect("nonzero selection node limit"),
        ItemLimit::new(MAX_ENTRIES).expect("nonzero selection container limit"),
        ByteLimit::new(MAX_DIRECTORY_BYTES).expect("nonzero selection string limit"),
        ByteLimit::new(BYTE_LIMIT as usize).expect("nonzero selection string aggregate"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|_| Error::InvalidRecord)?;
    deserializer.end().map_err(|_| Error::InvalidRecord)?;
    let record: SelectionRecord =
        serde_json::from_slice(bytes).map_err(|_| Error::InvalidRecord)?;
    if record.format_version != 1 {
        return Err(Error::UnsupportedFormat);
    }
    if record.installed.len() > MAX_ENTRIES
        || record
            .installed
            .iter()
            .any(|entry| entry.requires.len() > MAX_REQUIREMENTS)
        || record
            .installed
            .iter()
            .map(|entry| entry.requires.len())
            .sum::<usize>()
            > MAX_EDGES
    {
        return Err(Error::Graph);
    }
    let entries: Vec<_> = record
        .installed
        .into_iter()
        .map(|entry| Entry {
            installed: EditorSourceToolInstalledSelection {
                package: entry.package.as_ref().into(),
                directory: entry.directory.as_ref().into(),
                installation: entry.installation,
                receipt: ContentDigest::from_parts(entry.receipt.length, entry.receipt.hash),
                manifest: ContentDigest::from_parts(entry.manifest.length, entry.manifest.hash),
            },
            requirements: entry
                .requires
                .into_iter()
                .map(|binding| Binding {
                    package: binding.package.as_ref().into(),
                    installation: binding.installation,
                })
                .collect(),
        })
        .collect();
    let desired = record
        .desired
        .map(|desired| -> Result<Desired, Error> {
            let entry = entries
                .iter()
                .find(|entry| entry.installed.installation == desired.installation)
                .ok_or(Error::Graph)?;
            Ok(Desired {
                installed: entry.installed.clone(),
                enabled: desired.enabled,
            })
        })
        .transpose()?;
    Graph { entries, desired }.register(&[])
}

/// A canonical snapshot never grows its output buffer beyond the publication byte allowance.
struct RecordBytes {
    bytes: Vec<u8>,
    oversized: bool,
}

impl Write for RecordBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > BYTE_LIMIT as usize - self.bytes.len() {
            self.oversized = true;
            return Err(std::io::Error::other("source-tool selection byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
        // Another writer may create the directory. Its exact name and kind still need admission.
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
    RelativePath::new(name).expect("the engine-owned selection path is valid")
}

fn fixed_component(name: &str) -> RelativeComponent {
    RelativeComponent::new(name).expect("the engine-owned selection component is valid")
}

#[cfg(test)]
mod tests;
