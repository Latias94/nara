use std::{
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use nara_asset::AssetPath;

use crate::{AssetWatchError, AssetWatchPath};

pub(crate) fn native_watch_paths(
    root: &Path,
    paths: &[AssetWatchPath],
) -> Result<Vec<AssetWatchPath>, AssetWatchError> {
    if paths.is_empty() {
        return Err(AssetWatchError::NoWatchPaths);
    }
    let root = absolute_lexical(root)?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| AssetWatchError::Filesystem(error.to_string()))?;
    paths
        .iter()
        .map(|selected| {
            let path = if selected.path().is_absolute() {
                normalize_lexical(selected.path())
            } else {
                normalize_lexical(root.join(selected.path()))
            };
            let outside = || AssetWatchError::SourceOutsideRoot {
                path: path.clone(),
                root: root.clone(),
            };
            if !path.starts_with(&root) {
                return Err(outside());
            }
            let mut existing = path.as_path();
            loop {
                match existing.canonicalize() {
                    Ok(canonical) if canonical.starts_with(&canonical_root) => break,
                    Ok(_) => return Err(outside()),
                    Err(error) if error.kind() == ErrorKind::NotFound => {
                        existing = existing.parent().ok_or_else(outside)?;
                    }
                    Err(error) => return Err(AssetWatchError::Filesystem(error.to_string())),
                }
            }
            Ok(AssetWatchPath::new(path, selected.recursive()))
        })
        .collect()
}

pub(crate) fn local_path(root: &Path, path: &Path) -> Result<AssetPath, AssetWatchError> {
    let root = absolute_lexical(root)?;
    let path = if path.is_absolute() {
        normalize_lexical(path)
    } else {
        normalize_lexical(root.join(path))
    };
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| AssetWatchError::SourceOutsideRoot {
            path: path.clone(),
            root: root.clone(),
        })?;

    let mut segments = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => {
                let Some(segment) = segment.to_str() else {
                    return Err(AssetWatchError::NonUtf8Path(path.clone()));
                };
                segments.push(segment);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AssetWatchError::SourceOutsideRoot {
                    path: path.clone(),
                    root,
                });
            }
        }
    }

    AssetPath::new(segments.join("/")).map_err(AssetWatchError::InvalidLogicalPath)
}

pub(crate) fn absolute_lexical(path: &Path) -> Result<PathBuf, AssetWatchError> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| AssetWatchError::Filesystem(error.to_string()))?
            .join(path)
    };
    Ok(normalize_lexical(path))
}

pub(crate) fn same_lexical_path(left: &Path, right: &Path) -> Result<bool, AssetWatchError> {
    Ok(absolute_lexical(left)? == absolute_lexical(right)?)
}

fn normalize_lexical(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}
