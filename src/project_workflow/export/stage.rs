use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read, Write},
};

use nara_fs::{DirectoryCapability, FsError, FsOperation, RelativeComponent, RelativePath};
use nara_project::ProjectPath;

use super::{ProjectExportBuildReceipt, ProjectExportMemberSource, ProjectExportPlanReceipt};
use crate::project_host::StartupContentClosureReceipt;

#[derive(Debug, thiserror::Error)]
pub(super) enum ProjectExportStageError {
    #[error("failed to create export directory {path}")]
    CreateDirectory {
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

struct ExportDirectories {
    by_path: BTreeMap<String, DirectoryCapability>,
    creation_order: Vec<String>,
}

pub(super) fn stage_export_tree(
    staging: DirectoryCapability,
    plan: &ProjectExportPlanReceipt,
    startup_content: &StartupContentClosureReceipt,
    build: &ProjectExportBuildReceipt,
) -> Result<(), ProjectExportStageError> {
    let directories = create_directories(&staging, plan)?;
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
        let parent = directories
            .by_path
            .get(parent_path)
            .expect("the export planner creates every member parent");
        let mut source_reader = match source {
            ProjectExportMemberSource::Executable(target) => build
                .executable(*target)
                .reader()
                .map_err(|source| ProjectExportStageError::OpenSource {
                    path: member.path.clone(),
                    source,
                })?,
            ProjectExportMemberSource::StartupContent(source_path) => startup_content
                .project_root()
                .open_file(
                    &RelativePath::new(source_path.as_str())
                        .expect("the project-content owner publishes valid source paths"),
                )
                .and_then(|file| file.reader())
                .map_err(|source| ProjectExportStageError::OpenSource {
                    path: source_path.clone(),
                    source,
                })?,
        };
        let target_file = match member.kind {
            super::ProjectExportMemberKind::HeadlessExecutable
            | super::ProjectExportMemberKind::DesktopExecutable => {
                parent.create_child_executable_file(&leaf)
            }
            super::ProjectExportMemberKind::StartupContent(_) => parent.create_child_file(&leaf),
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
    for path in directories.creation_order.iter().rev() {
        let directory = directories
            .by_path
            .get(path)
            .expect("every created export directory remains owned until synchronization");
        sync_directory(directory).map_err(|source| ProjectExportStageError::SyncDirectory {
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
) -> Result<ExportDirectories, ProjectExportStageError> {
    let mut paths = BTreeSet::new();
    for member in &plan.members {
        let components = member.path.as_str().split('/').collect::<Vec<_>>();
        for depth in 1..components.len() {
            paths.insert(components[..depth].join("/"));
        }
    }
    let mut directories = BTreeMap::new();
    let mut creation_order = Vec::with_capacity(paths.len());
    for path in paths {
        let (parent_path, leaf) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let leaf = RelativeComponent::new(leaf)
            .expect("the export planner creates valid directory components");
        let parent = if parent_path.is_empty() {
            staging
        } else {
            directories
                .get(parent_path)
                .expect("parent export directories are ordered before children")
        };
        let directory = parent.create_child_directory(&leaf).map_err(|source| {
            ProjectExportStageError::CreateDirectory {
                path: ProjectPath::new(path.clone())
                    .expect("the export planner creates valid directory paths"),
                source,
            }
        })?;
        creation_order.push(path.clone());
        directories.insert(path, directory);
    }
    Ok(ExportDirectories {
        by_path: directories,
        creation_order,
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
