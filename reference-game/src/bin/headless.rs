use std::{ffi::OsString, fs::File, io, path::Path, process::ExitCode};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FileCapability, FsError, FsOperation,
        HostCapabilityOptions, RelativePath, TrustMode,
    },
    project_host::ProjectCandidateError,
};
use nara_reference_game::{ReferenceGameError, TracerSnapshot, run_headless_ticks_from_manifest};

const MANIFEST_OVERRIDE: &str = "NARA_REFERENCE_GAME_MANIFEST";

fn main() -> ExitCode {
    match run() {
        Ok(snapshot) => {
            println!(
                "tick={} enemy_hp={}",
                snapshot.tick, snapshot.enemy_hit_points
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<TracerSnapshot, ReferenceGameError> {
    let manifest = open_manifest()?;
    run_headless_ticks_from_manifest(&manifest, None, 3)
}

fn open_manifest() -> Result<FileCapability, ProjectCandidateError> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = host_directory(project_root).map_err(|source| {
        ProjectCandidateError::from_manifest_authority(FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        })
    })?;
    let root = DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .map_err(ProjectCandidateError::from_manifest_authority)?;
    let name = std::env::var_os(MANIFEST_OVERRIDE).unwrap_or_else(|| OsString::from("nara.toml"));
    let relative = RelativePath::new(Path::new(&name))
        .map_err(|error| ProjectCandidateError::from_manifest_authority(FsError::from(error)))?;
    root.open_file(&relative)
        .map_err(ProjectCandidateError::from_manifest_authority)
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
}
