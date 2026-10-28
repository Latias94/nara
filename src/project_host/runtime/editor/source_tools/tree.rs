//! Exact finite tree observation shared by installation and native-template publication.

use std::collections::BTreeMap;

use nara_fs::{DirectoryCapability, DirectoryEnumerationLimits, FileKind, FsError, RelativePath};

pub(super) enum FileTreeError {
    Changed(String),
    Filesystem { path: String, source: FsError },
}

/// The calling format already bounds and validates its complete portable member list.
pub(super) fn verify<'a>(
    root: &DirectoryCapability,
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<(), FileTreeError> {
    verify_observed(root, paths, |_, _| true)
}

/// Compares enumerated regular leaves with observations whose caller still holds live handles.
pub(super) fn verify_observed<'a>(
    root: &DirectoryCapability,
    paths: impl IntoIterator<Item = &'a str>,
    mut same_leaf: impl FnMut(&str, Option<nara_fs::FileIdentity>) -> bool,
) -> Result<(), FileTreeError> {
    let mut expected = BTreeMap::new();
    for path in paths {
        expected.insert(path.to_owned(), FileKind::Regular);
        for (separator, _) in path.match_indices('/') {
            expected.insert(path[..separator].to_owned(), FileKind::Directory);
        }
    }
    let limit = expected.len();
    let mut consumed = 0;
    let mut pending = vec![(root.share(), String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        let filesystem = |source| FileTreeError::Filesystem {
            path: if prefix.is_empty() {
                "file tree".to_owned()
            } else {
                prefix.clone()
            },
            source,
        };
        let mut scan = directory
            .begin_child_enumeration(DirectoryEnumerationLimits::new(
                limit.saturating_sub(consumed),
                512,
                0,
            ))
            .map_err(filesystem)?;
        loop {
            let batch = scan.next_batch(64).map_err(filesystem)?;
            if !batch.rejected_entry_facts().is_empty() {
                return Err(FileTreeError::Changed(prefix));
            }
            for entry in batch.entries() {
                consumed += 1;
                let name = entry
                    .name()
                    .to_str()
                    .ok_or_else(|| FileTreeError::Changed(prefix.clone()))?;
                let path = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                };
                if expected.remove(&path) != Some(entry.kind()) {
                    return Err(FileTreeError::Changed(path));
                }
                if entry.kind() == FileKind::Regular && !same_leaf(&path, entry.identity()) {
                    return Err(FileTreeError::Changed(path));
                }
                if entry.kind() == FileKind::Directory {
                    let child = root
                        .open_directory_exact(&RelativePath::new(&path).map_err(|source| {
                            FileTreeError::Filesystem {
                                path: path.clone(),
                                source: source.into(),
                            }
                        })?)
                        .map_err(|source| FileTreeError::Filesystem {
                            path: path.clone(),
                            source,
                        })?;
                    if Some(child.identity()) != entry.identity() {
                        return Err(FileTreeError::Changed(path));
                    }
                    pending.push((child, path));
                }
            }
            if batch.is_complete() {
                break;
            }
        }
    }
    if expected.is_empty() {
        Ok(())
    } else {
        Err(FileTreeError::Changed("file tree".to_owned()))
    }
}
