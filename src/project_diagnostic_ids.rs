use nara_fs::FsOperation;

pub(crate) const fn fs_operation_id(operation: FsOperation) -> &'static str {
    match operation {
        FsOperation::InspectHandle => "inspect-handle",
        FsOperation::OpenDirectory => "open-directory",
        FsOperation::OpenFile => "open-file",
        FsOperation::ReadDirectory => "read-directory",
        FsOperation::CreateTemporary => "create-temporary",
        FsOperation::RemoveTemporary => "remove-temporary",
        FsOperation::Rename => "rename",
        FsOperation::Unlink => "unlink",
        FsOperation::Replace => "replace",
        FsOperation::SyncFile => "sync-file",
        FsOperation::SyncDirectory => "sync-directory",
        FsOperation::Lock => "lock",
        FsOperation::Unlock => "unlock",
        FsOperation::CloneHandle => "clone-handle",
        FsOperation::Read => "read",
    }
}

pub(crate) fn io_error_kind_id(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "not-found",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        std::io::ErrorKind::AlreadyExists => "already-exists",
        std::io::ErrorKind::InvalidInput => "invalid-input",
        std::io::ErrorKind::InvalidData => "invalid-data",
        std::io::ErrorKind::TimedOut => "timed-out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::UnexpectedEof => "unexpected-eof",
        std::io::ErrorKind::OutOfMemory => "out-of-memory",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_operation_ids_are_stable_and_complete() {
        let expected = [
            (FsOperation::InspectHandle, "inspect-handle"),
            (FsOperation::OpenDirectory, "open-directory"),
            (FsOperation::OpenFile, "open-file"),
            (FsOperation::ReadDirectory, "read-directory"),
            (FsOperation::CreateTemporary, "create-temporary"),
            (FsOperation::RemoveTemporary, "remove-temporary"),
            (FsOperation::Rename, "rename"),
            (FsOperation::Unlink, "unlink"),
            (FsOperation::Replace, "replace"),
            (FsOperation::SyncFile, "sync-file"),
            (FsOperation::SyncDirectory, "sync-directory"),
            (FsOperation::Lock, "lock"),
            (FsOperation::Unlock, "unlock"),
            (FsOperation::CloneHandle, "clone-handle"),
            (FsOperation::Read, "read"),
        ];

        for (operation, id) in expected {
            assert_eq!(fs_operation_id(operation), id);
        }
    }

    #[test]
    fn io_error_kind_ids_use_one_bounded_classifier() {
        let expected = [
            (std::io::ErrorKind::NotFound, "not-found"),
            (std::io::ErrorKind::PermissionDenied, "permission-denied"),
            (std::io::ErrorKind::AlreadyExists, "already-exists"),
            (std::io::ErrorKind::InvalidInput, "invalid-input"),
            (std::io::ErrorKind::InvalidData, "invalid-data"),
            (std::io::ErrorKind::TimedOut, "timed-out"),
            (std::io::ErrorKind::Interrupted, "interrupted"),
            (std::io::ErrorKind::UnexpectedEof, "unexpected-eof"),
            (std::io::ErrorKind::OutOfMemory, "out-of-memory"),
            (std::io::ErrorKind::ConnectionReset, "other"),
        ];

        for (kind, id) in expected {
            assert_eq!(io_error_kind_id(kind), id);
        }
    }
}
