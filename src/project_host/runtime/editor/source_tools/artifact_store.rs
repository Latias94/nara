//! Fixed project-owned artifact namespace; paths alone never authorize disposal.

use nara_fs::{
    CapabilityRights, DirectoryCapability, FsError, FsOperation, RelativeComponent, RelativePath,
};

pub(super) const PATH: &str = ".nara/source-tools/artifacts";
pub(super) const PREFIX: &str = ".nara/source-tools/artifacts/";

pub(super) fn validate_path(path: &str) -> bool {
    path.strip_prefix(PREFIX)
        .is_some_and(|name| name.is_ascii() && RelativeComponent::new(name).is_ok())
}

/// Source admission reserves the complete namespace, including case aliases on native filesystems.
pub(super) fn contains_path(path: &str) -> bool {
    path.eq_ignore_ascii_case(PATH)
        || path
            .get(..PREFIX.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(PREFIX))
}

/// Creates only the fixed store parents during explicit publication, never during preparation.
/// Failure retains any created directories; no recursive cleanup or artifact ownership is implied.
pub(super) fn ensure_parent(root: &DirectoryCapability) -> Result<DirectoryCapability, FsError> {
    if root.options().rights() != CapabilityRights::ReadWrite {
        return Err(FsError::ReadOnlyCapability {
            operation: FsOperation::CreateDirectory,
        });
    }
    let mut parent = root.share();
    for name in [".nara", "source-tools", "artifacts"] {
        let path = RelativePath::new(name).expect("the fixed artifact store component is valid");
        match parent.open_directory_exact(&path) {
            Ok(directory) => {
                parent = directory;
                continue;
            }
            Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let name =
            RelativeComponent::new(name).expect("the fixed artifact store component is valid");
        let created = match parent.create_child_directory(&name) {
            Ok(directory) => Some(directory),
            Err(FsError::AlreadyExists { .. }) => None,
            Err(error) => return Err(error),
        };
        let opened = parent.open_directory_exact(&path)?;
        if let Some(created) = created
            && created.identity() != opened.identity()
        {
            return Err(FsError::IdentityMismatch {
                expected: created.identity(),
                actual: opened.identity(),
            });
        }
        parent = opened;
    }
    Ok(parent)
}
