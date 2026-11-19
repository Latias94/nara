use std::{
    collections::BTreeSet,
    io::{self, Cursor, Read, Write},
};

use nara_fs::{
    CapabilityReader, DirectoryCapability, FsError, FsOperation, RelativeComponent, RelativePath,
};
use nara_project::ProjectPath;

use super::{ProjectExportBuildReceipt, ProjectExportMemberSource, ProjectExportPlanReceipt};

#[derive(Debug, thiserror::Error)]
pub(super) enum ProjectExportStageError {
    #[error("failed to create export directory {path}")]
    CreateDirectory {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to open staged export directory {path}")]
    OpenDirectory {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to open export member source {path}")]
    OpenSource {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to create export member {path}")]
    CreateFile {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to stream export member {path}")]
    CopyFile {
        path: ProjectPath,
        #[source]
        source: io::Error,
    },
    #[error("export member {path} changed length while staging")]
    SourceLengthChanged { path: ProjectPath },
    #[error("failed to synchronize export member {path}")]
    SyncFile {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to verify staged export member {path}")]
    VerifyFile {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to synchronize export directory {path}")]
    SyncDirectory {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("failed to synchronize the export staging root")]
    SyncRoot {
        #[source]
        source: FsError,
    },
}

enum ExportMemberReader<'a> {
    Executable(Cursor<&'a [u8]>),
    Content(CapabilityReader<'a>),
}

impl Read for ExportMemberReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Executable(reader) => reader.read(buffer),
            Self::Content(reader) => reader.read(buffer),
        }
    }
}

pub(super) fn stage_export_tree(
    staging: DirectoryCapability,
    plan: &ProjectExportPlanReceipt,
    project_root: &DirectoryCapability,
    build: &ProjectExportBuildReceipt,
) -> Result<(), ProjectExportStageError> {
    let directories = create_directories(&staging, plan)?;
    let mut open_parent = None::<(&str, DirectoryCapability)>;
    for (member, source) in plan.members.iter().zip(plan.sources.iter()) {
        let target = RelativePath::new(member.path.as_str())
            .expect("the export planner publishes valid target paths");
        let (parent_path, leaf) = member
            .path
            .as_str()
            .rsplit_once('/')
            .expect("the closed export layout places every member below a directory");
        let leaf = RelativeComponent::new(leaf)
            .expect("the export planner publishes valid target components");
        if open_parent
            .as_ref()
            .is_none_or(|(path, _)| *path != parent_path)
        {
            open_parent = Some((parent_path, open_staged_directory(&staging, parent_path)?));
        }
        let (_, parent) = open_parent
            .as_ref()
            .expect("the current member parent is open");
        let content_source;
        let mut source_reader = match source {
            ProjectExportMemberSource::Executable(target) => {
                ExportMemberReader::Executable(Cursor::new(build.executable_bytes(*target)))
            }
            ProjectExportMemberSource::Content(source_path) => {
                content_source = project_root
                    .open_file(
                        &RelativePath::new(source_path.as_str())
                            .expect("the project-content owner publishes valid source paths"),
                    )
                    .map_err(|source| ProjectExportStageError::OpenSource {
                        path: source_path.clone(),
                        source,
                    })?;
                ExportMemberReader::Content(content_source.reader().map_err(|source| {
                    ProjectExportStageError::OpenSource {
                        path: source_path.clone(),
                        source,
                    }
                })?)
            }
        };
        let target_file = match member.kind {
            super::ProjectExportMemberKind::HeadlessExecutable
            | super::ProjectExportMemberKind::DesktopExecutable => {
                parent.create_child_executable_file(&leaf)
            }
            super::ProjectExportMemberKind::Content(_) => parent.create_child_file(&leaf),
        };
        let mut target_file =
            target_file.map_err(|source| ProjectExportStageError::CreateFile {
                path: member.path.clone(),
                source,
            })?;
        let copied = {
            let mut limited = (&mut source_reader).take(member.digest.length().saturating_add(1));
            let mut writer =
                target_file
                    .writer()
                    .map_err(|source| ProjectExportStageError::CreateFile {
                        path: member.path.clone(),
                        source,
                    })?;
            let copied = io::copy(&mut limited, &mut writer).map_err(|source| {
                ProjectExportStageError::CopyFile {
                    path: member.path.clone(),
                    source,
                }
            })?;
            writer
                .flush()
                .map_err(|source| ProjectExportStageError::CopyFile {
                    path: member.path.clone(),
                    source,
                })?;
            copied
        };
        if copied != member.digest.length() {
            return Err(ProjectExportStageError::SourceLengthChanged {
                path: member.path.clone(),
            });
        }
        target_file
            .sync()
            .map_err(|source| ProjectExportStageError::SyncFile {
                path: member.path.clone(),
                source,
            })?;
        staging
            .open_file(&target)
            .and_then(|file| file.verify_digest(member.digest, member.digest.length()))
            .map_err(|source| ProjectExportStageError::VerifyFile {
                path: member.path.clone(),
                source,
            })?;
    }
    drop(open_parent);
    for path in directories.iter().rev() {
        let directory = open_staged_directory(&staging, path)?;
        sync_directory(&directory).map_err(|source| ProjectExportStageError::SyncDirectory {
            path: ProjectPath::new(path.clone())
                .expect("the export planner creates valid directory paths"),
            source,
        })?;
    }
    sync_directory(&staging).map_err(|source| ProjectExportStageError::SyncRoot { source })?;
    Ok(())
}

fn create_directories(
    staging: &DirectoryCapability,
    plan: &ProjectExportPlanReceipt,
) -> Result<BTreeSet<String>, ProjectExportStageError> {
    let mut paths = BTreeSet::new();
    for directory in plan.runtime_directories() {
        let target = format!("project/{directory}");
        let components = target.split('/').collect::<Vec<_>>();
        for depth in 1..=components.len() {
            paths.insert(components[..depth].join("/"));
        }
    }
    for member in &plan.members {
        let components = member.path.as_str().split('/').collect::<Vec<_>>();
        for depth in 1..components.len() {
            paths.insert(components[..depth].join("/"));
        }
    }
    for path in &paths {
        let (parent_path, leaf) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let leaf = RelativeComponent::new(leaf)
            .expect("the export planner creates valid directory components");
        let parent = open_staged_directory(staging, parent_path)?;
        parent.create_child_directory(&leaf).map_err(|source| {
            ProjectExportStageError::CreateDirectory {
                path: ProjectPath::new(path.clone())
                    .expect("the export planner creates valid directory paths"),
                source,
            }
        })?;
    }
    Ok(paths)
}

fn open_staged_directory(
    staging: &DirectoryCapability,
    path: &str,
) -> Result<DirectoryCapability, ProjectExportStageError> {
    if path.is_empty() {
        return Ok(staging.share());
    }
    let target = RelativePath::new(path).expect("the export planner creates valid directory paths");
    staging
        .open_directory(&target)
        .map_err(|source| ProjectExportStageError::OpenDirectory {
            path: ProjectPath::new(path).expect("the export directory is project-relative"),
            source,
        })
}

fn sync_directory(directory: &DirectoryCapability) -> Result<(), FsError> {
    match directory.sync() {
        Ok(_)
        | Err(FsError::Unsupported {
            operation: FsOperation::SyncDirectory,
            ..
        }) => Ok(()),
        Err(source) => Err(source),
    }
}
