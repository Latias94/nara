use std::{
    io::{Seek, SeekFrom},
    path::Path,
};

use nara::fs::{
    ContentDigest, DirectoryCapability, FileCapability, FsError, FsOperation, LockMode,
    LockedFileCapability, RelativeComponent, RelativePath,
};

use super::{Binding, SaveIoError, SaveSlot, open_path, optional_file};

const SERVER_MARKER_PREFIX: &str = ".two-level-lock-";

enum ServerOwnership {
    Staged,
    Active(LockedFileCapability),
    Closing(LockedFileCapability),
    Closed,
}

pub(super) struct ServerSlot {
    reader: DirectoryCapability,
    target: RelativePath,
    marker_path: RelativePath,
    marker: FileCapability,
    ownership: ServerOwnership,
}

impl SaveSlot {
    /// Acquires the stable sibling marker before observing the checkpoint.
    pub(crate) fn open_server(path: &Path) -> Result<Self, SaveIoError> {
        Self::open_coordinated(path, None)
    }

    /// Retains the successor's original observations while the source is still live.
    pub(crate) fn prepare_server_handoff(
        path: &Path,
        expected: [u8; 32],
    ) -> Result<Self, SaveIoError> {
        Self::open_coordinated(path, Some(expected))
    }

    fn open_coordinated(
        path: &Path,
        expected_handoff: Option<[u8; 32]>,
    ) -> Result<Self, SaveIoError> {
        let (parent, target) = open_path(path).map_err(|error| match error {
            SaveIoError::File(error) => server_file_error(error),
            error => error,
        })?;
        let leaf = target.to_str().ok_or(SaveIoError::InvalidPath)?;
        if leaf
            .as_bytes()
            .get(..SERVER_MARKER_PREFIX.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(SERVER_MARKER_PREFIX.as_bytes()))
        {
            return Err(SaveIoError::InvalidPath);
        }
        let marker_name = RelativeComponent::new(format!("{SERVER_MARKER_PREFIX}{leaf}"))
            .map_err(|_| SaveIoError::InvalidPath)?;
        let marker_path = RelativePath::from_components([marker_name.clone()])
            .map_err(|_| SaveIoError::InvalidPath)?;
        let target_path = RelativePath::from_components([target.clone()])
            .map_err(|_| SaveIoError::InvalidPath)?;
        let reader = parent.strict_read_only().map_err(server_file_error)?;
        let marker =
            match optional_file(reader.open_file_exact(&marker_path)).map_err(server_file_error)? {
                Some(marker) => marker,
                None if expected_handoff.is_some() => return Err(SaveIoError::Conflict),
                None => {
                    let created = match parent.create_child_file(&marker_name) {
                        Ok(created) => Some(created),
                        Err(FsError::AlreadyExists { .. }) => None,
                        Err(error) => return Err(server_file_error(error)),
                    };
                    let marker = optional_file(reader.open_file_exact(&marker_path))
                        .map_err(server_file_error)?
                        .ok_or(SaveIoError::Conflict)?;
                    if created.is_some_and(|file| file.identity() != marker.identity()) {
                        return Err(SaveIoError::Conflict);
                    }
                    marker
                }
            };
        let mut server = ServerSlot {
            reader,
            target: target_path,
            marker_path,
            marker,
            ownership: ServerOwnership::Staged,
        };
        if expected_handoff.is_none() {
            server.ownership = ServerOwnership::Active(server.lock_marker()?);
        }
        let binding = Binding::observe(server.observe_target()?)?;
        let slot = Self::with_binding(parent, target, binding, Some(server));
        if let Some(expected) = expected_handoff {
            if slot.handoff_stamp()? != expected {
                return Err(SaveIoError::Conflict);
            }
        } else {
            slot.verify_server_access()?;
        }
        Ok(slot)
    }

    /// Revalidates the retained checkpoint and marker for a live local handoff.
    pub(crate) fn handoff_stamp(&self) -> Result<[u8; 32], SaveIoError> {
        let server = self
            .server
            .as_ref()
            .filter(|server| {
                matches!(
                    server.ownership,
                    ServerOwnership::Staged | ServerOwnership::Active(_)
                )
            })
            .ok_or(SaveIoError::HandoffUnavailable)?;
        if self.cleanup_pending() {
            return Err(SaveIoError::CleanupPending);
        }
        if self.uncertain {
            return Err(SaveIoError::Uncertain);
        }
        server.verify_baseline(&self.binding)?;

        let leaf = self.target.to_str().ok_or(SaveIoError::InvalidPath)?;
        let marker_stamp = server
            .marker
            .local_object_stamp()
            .map_err(server_file_error)?;
        let mut bytes = b"two-level-game.server-checkpoint.v1\0".to_vec();
        bytes.extend_from_slice(&marker_stamp);
        bytes.extend_from_slice(&(leaf.len() as u64).to_le_bytes());
        bytes.extend_from_slice(leaf.as_bytes());
        match &self.binding {
            Binding::Missing => bytes.push(0),
            Binding::Existing { file, digest } => {
                bytes.push(1);
                bytes.extend_from_slice(&file.local_object_stamp().map_err(server_file_error)?);
                bytes.extend_from_slice(&digest.length().to_le_bytes());
                bytes.extend_from_slice(digest.as_bytes());
            }
        }
        Ok(*ContentDigest::of_bytes(&bytes).as_bytes())
    }

    /// Claims the original marker and retains the staged observations on failure.
    pub(crate) fn claim_server_handoff(&mut self) -> Result<(), SaveIoError> {
        let server = self
            .server
            .as_mut()
            .filter(|server| matches!(server.ownership, ServerOwnership::Staged))
            .ok_or(SaveIoError::HandoffUnavailable)?;
        let lock = server.lock_marker()?;
        server.verify_baseline(&self.binding)?;
        server.ownership = ServerOwnership::Active(lock);
        Ok(())
    }

    /// Retains pending cleanup or unlock custody until the caller can drop this slot.
    pub(crate) fn retry_close(&mut self) -> bool {
        self.retry_cleanup() && self.server.as_mut().is_none_or(ServerSlot::retry_close)
    }
}

impl ServerSlot {
    pub(super) fn is_active(&self) -> bool {
        matches!(self.ownership, ServerOwnership::Active(_))
    }

    pub(super) fn verify_access(&self) -> Result<(), SaveIoError> {
        if !self.is_active() {
            return Err(SaveIoError::HandoffUnavailable);
        }
        self.verified_marker()?;
        Ok(())
    }

    pub(super) fn open_target(&self) -> Result<FileCapability, FsError> {
        self.reader.open_file_exact(&self.target)
    }

    pub(super) fn observe_target(&self) -> Result<Option<FileCapability>, SaveIoError> {
        optional_file(self.open_target()).map_err(server_file_error)
    }

    fn verified_marker(&self) -> Result<FileCapability, SaveIoError> {
        let file = optional_file(self.reader.open_file_exact(&self.marker_path))
            .map_err(server_file_error)?
            .ok_or(SaveIoError::Conflict)?;
        if file.identity() != self.marker.identity() {
            return Err(SaveIoError::Conflict);
        }
        let length = file
            .reader()?
            .seek(SeekFrom::End(0))
            .map_err(|source| FsError::Io {
                operation: FsOperation::Read,
                source,
            })?;
        if length != 0 {
            return Err(SaveIoError::Conflict);
        }
        Ok(file)
    }

    fn lock_marker(&self) -> Result<LockedFileCapability, SaveIoError> {
        self.verified_marker()?
            .try_lock(LockMode::Exclusive)
            .map_err(|failure| server_file_error(failure.into_parts().0))
    }

    fn verify_baseline(&self, binding: &Binding) -> Result<(), SaveIoError> {
        drop(binding.verify(self.observe_target()?)?);
        self.verified_marker()?;
        Ok(())
    }

    fn retry_close(&mut self) -> bool {
        let lock = match std::mem::replace(&mut self.ownership, ServerOwnership::Closed) {
            ServerOwnership::Active(lock) | ServerOwnership::Closing(lock) => lock,
            ServerOwnership::Staged | ServerOwnership::Closed => return true,
        };
        match lock.release() {
            Ok(_) => true,
            Err(failure) => {
                self.ownership = ServerOwnership::Closing(failure.into_locked());
                false
            }
        }
    }
}

pub(super) fn server_file_error(error: FsError) -> SaveIoError {
    match error {
        FsError::Unsupported { .. }
        | FsError::Unproven { .. }
        | FsError::IdentityUnavailable
        | FsError::DirectoryEnumerationEntryLimitExceeded { .. } => {
            SaveIoError::UnsupportedGuarantee
        }
        FsError::ExactSpellingMismatch { .. }
        | FsError::CapabilityIdentityDrift { .. }
        | FsError::MultipleLinks { .. }
        | FsError::NotRegularFile
        | FsError::ReparsePoint { .. }
        | FsError::SymbolicLinkTraversal
        | FsError::CrossVolume => SaveIoError::Conflict,
        _ => SaveIoError::File(error),
    }
}
