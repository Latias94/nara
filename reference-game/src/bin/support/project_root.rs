use std::{fs::File, io, path::Path};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    project_host::ProjectCandidateError,
};

pub fn open_project_root() -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = host_directory(project_root).map_err(|source| {
        Box::new(ProjectCandidateError::from_manifest_authority(
            FsError::Io {
                operation: FsOperation::OpenDirectory,
                source,
            },
        ))
    })?;
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .map_err(|error| Box::new(ProjectCandidateError::from_manifest_authority(error)))
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    #[cfg(unix)]
    {
        File::open(path)
    }

    #[cfg(not(any(windows, unix)))]
    {
        File::open(path)
    }
}
