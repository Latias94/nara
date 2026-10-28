use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
};

use crate::{
    DirectoryCapability, FileIdentity, FsError, FsOperation, RelativeComponent,
    capability::CreatedDirectoryChild,
};

/// A failed exclusive directory creation and custody of any child it created.
///
/// A failure before creation has no custody. Once exclusive creation succeeds, custody is
/// retained even if opening or validating the child fails. Dropping this failure does not remove
/// the child or retry the operation.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct DirectoryCreateFailure {
    #[source]
    error: FsError,
    created: Option<CreatedDirectoryCustody>,
}

impl DirectoryCreateFailure {
    #[must_use]
    pub const fn error(&self) -> &FsError {
        &self.error
    }

    /// Returns custody only when this operation actually created an exclusive child.
    #[must_use]
    pub const fn created(&self) -> Option<&CreatedDirectoryCustody> {
        self.created.as_ref()
    }

    #[must_use]
    pub fn into_parts(self) -> (FsError, Option<CreatedDirectoryCustody>) {
        (self.error, self.created)
    }
}

/// Retains the parent, original name and any available handle of an unverified created child.
///
/// This is evidence of a creation result, not directory access or deletion authority. The name
/// does not prove that the current namespace still refers to the created child. An unavailable
/// handle or identity does not undo the fact of creation. Dropping custody closes its handles
/// without removing anything or retrying validation.
pub struct CreatedDirectoryCustody {
    parent: DirectoryCapability,
    name: RelativeComponent,
    file: Option<File>,
    identity: Option<FileIdentity>,
}

impl CreatedDirectoryCustody {
    /// Returns the original relative name for diagnostics, not authorization.
    #[must_use]
    pub const fn name(&self) -> &RelativeComponent {
        &self.name
    }

    /// Returns the identity observed before validation failed, when it was proven.
    ///
    /// Identity alone does not establish that the object passed directory validation.
    #[must_use]
    pub const fn identity(&self) -> Option<FileIdentity> {
        self.identity
    }

    /// Reports whether the created object's native handle is still retained.
    ///
    /// A retained handle does not grant traversal authority or prove successful validation.
    #[must_use]
    pub const fn has_handle(&self) -> bool {
        self.file.is_some()
    }
}

impl Debug for CreatedDirectoryCustody {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreatedDirectoryCustody")
            .field("parent_identity", &self.parent.identity())
            .field("identity", &self.identity)
            .field("has_handle", &self.has_handle())
            .finish_non_exhaustive()
    }
}

pub(crate) fn create_child_directory_retained(
    parent: &DirectoryCapability,
    name: &RelativeComponent,
) -> Result<DirectoryCapability, DirectoryCreateFailure> {
    let created = parent
        .create_child_directory_retaining_unverified(name)
        .map_err(|error| DirectoryCreateFailure {
            error,
            created: None,
        })?;
    let (file, identity, error) = match created {
        CreatedDirectoryChild::Verified(directory) => return Ok(directory),
        CreatedDirectoryChild::CreatedButUnopened { error } => (None, None, error),
        CreatedDirectoryChild::Unverified {
            file,
            identity,
            error,
        } => (Some(file), identity, error),
    };
    Err(DirectoryCreateFailure {
        error: FsError::exclusive_child_unverified(FsOperation::CreateDirectory, error),
        created: Some(CreatedDirectoryCustody {
            parent: parent.share(),
            name: name.clone(),
            file,
            identity,
        }),
    })
}

#[cfg(all(test, any(unix, windows)))]
mod tests;
