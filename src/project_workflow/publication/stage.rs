use std::io::{self, Write};

use nara_fs::{DirectoryCapability, FsError, FsOperation, RelativeComponent, RelativePath};

use super::super::starter::{
    ProjectCreationPlan, ProjectDirectory, ProjectMember, ProjectMemberPlan,
    STARTER_ACTION_MAP_PATH,
};

#[derive(Debug, thiserror::Error)]
pub enum ProjectTreeStageError {
    #[error("failed to create starter directory {directory:?}")]
    CreateDirectory {
        directory: ProjectDirectory,
        #[source]
        source: FsError,
    },
    #[error("failed to create starter member {member:?}")]
    CreateFile {
        member: ProjectMember,
        #[source]
        source: FsError,
    },
    #[error("failed to open the writer for starter member {member:?}")]
    OpenWriter {
        member: ProjectMember,
        #[source]
        source: FsError,
    },
    #[error("failed to write starter member {member:?}")]
    WriteFile {
        member: ProjectMember,
        #[source]
        source: io::Error,
    },
    #[error("failed to synchronize starter member {member:?}")]
    SyncFile {
        member: ProjectMember,
        #[source]
        source: FsError,
    },
    #[error("failed to verify starter member {member:?}")]
    VerifyFile {
        member: ProjectMember,
        #[source]
        source: FsError,
    },
    #[error("failed to synchronize starter directory {directory:?}")]
    SyncDirectory {
        directory: Option<ProjectDirectory>,
        #[source]
        source: FsError,
    },
}

pub(super) fn stage_project_tree(
    staging: DirectoryCapability,
    plan: &ProjectCreationPlan,
) -> Result<(), ProjectTreeStageError> {
    let mut directories = Vec::with_capacity(plan.directories().len());
    for &directory in plan.directories() {
        let path = directory_path(directory);
        let parent = match directory {
            ProjectDirectory::SourceBinaries => {
                directory_in(&directories, ProjectDirectory::Source)
            }
            _ => &staging,
        };
        let handle = parent
            .create_child_directory(leaf_name(&path))
            .map_err(|source| ProjectTreeStageError::CreateDirectory { directory, source })?;
        directories.push((directory, handle));
    }
    for member in plan.members() {
        let (directory, name) = member_location(member.member());
        let parent = directory.map_or(&staging, |directory| directory_in(&directories, directory));
        write_member(parent, name, member)?;
    }
    for member in plan.members() {
        verify_staged_member(&staging, member)?;
    }
    for (directory, handle) in directories.iter().rev() {
        sync_directory(handle, Some(*directory))?;
    }
    sync_directory(&staging, None)?;
    Ok(())
}

fn directory_in(
    directories: &[(ProjectDirectory, DirectoryCapability)],
    directory: ProjectDirectory,
) -> &DirectoryCapability {
    directories
        .iter()
        .find_map(|(kind, handle)| (*kind == directory).then_some(handle))
        .expect("the closed starter plan includes each parent directory before its children")
}

fn write_member(
    parent: &DirectoryCapability,
    name: &str,
    member: &ProjectMemberPlan,
) -> Result<(), ProjectTreeStageError> {
    let mut file = parent
        .create_child_file(&component(name))
        .map_err(|source| ProjectTreeStageError::CreateFile {
            member: member.member(),
            source,
        })?;
    {
        let mut writer = file
            .writer()
            .map_err(|source| ProjectTreeStageError::OpenWriter {
                member: member.member(),
                source,
            })?;
        writer
            .write_all(member.bytes())
            .and_then(|()| writer.flush())
            .map_err(|source| ProjectTreeStageError::WriteFile {
                member: member.member(),
                source,
            })?;
    }
    file.sync()
        .map_err(|source| ProjectTreeStageError::SyncFile {
            member: member.member(),
            source,
        })?;
    Ok(())
}

fn verify_staged_member(
    root: &DirectoryCapability,
    member: &ProjectMemberPlan,
) -> Result<(), ProjectTreeStageError> {
    let file = root
        .open_file(&member_path(member.member()))
        .map_err(|source| ProjectTreeStageError::VerifyFile {
            member: member.member(),
            source,
        })?;
    file.verify_digest(member.digest(), member.digest().length())
        .map_err(|source| ProjectTreeStageError::VerifyFile {
            member: member.member(),
            source,
        })
}

fn sync_directory(
    directory: &DirectoryCapability,
    project_directory: Option<ProjectDirectory>,
) -> Result<(), ProjectTreeStageError> {
    match directory.sync() {
        Ok(_)
        | Err(FsError::Unsupported {
            operation: FsOperation::SyncDirectory,
            ..
        }) => Ok(()),
        Err(source) => Err(ProjectTreeStageError::SyncDirectory {
            directory: project_directory,
            source,
        }),
    }
}

pub(super) fn member_path(member: ProjectMember) -> RelativePath {
    let (directory, name) = member_location(member);
    if let Some(directory) = directory {
        RelativePath::new(format!("{}/{name}", directory_name(directory)))
    } else {
        RelativePath::new(name)
    }
    .expect("the closed starter inventory uses valid relative paths")
}

fn member_location(member: ProjectMember) -> (Option<ProjectDirectory>, &'static str) {
    use ProjectDirectory::{Input, Scenes, Schema, Source, SourceBinaries};

    let (directory, name) = match member {
        ProjectMember::CargoManifest => return (None, "Cargo.toml"),
        ProjectMember::ProjectManifest => return (None, "nara.toml"),
        ProjectMember::AuthorGuide => return (None, "README.md"),
        ProjectMember::BuildScript => return (None, "build.rs"),
        ProjectMember::AuthoringSchemaBundle => (Schema, "authoring.bundle.json"),
        ProjectMember::LibrarySource => (Source, "lib.rs"),
        ProjectMember::GameplaySource => (Source, "gameplay.rs"),
        ProjectMember::HeadlessSource => (SourceBinaries, "headless.rs"),
        ProjectMember::DesktopSource => (SourceBinaries, "desktop.rs"),
        ProjectMember::CookSource => (SourceBinaries, "cook.rs"),
        ProjectMember::PlayerSource => (SourceBinaries, "player.rs"),
        ProjectMember::StartupScene => (Scenes, "startup.scene.json"),
        ProjectMember::StartupSceneMetadata => (Scenes, "startup.scene.json.meta"),
        ProjectMember::InputActionMap => {
            (Input, STARTER_ACTION_MAP_PATH.rsplit_once('/').unwrap().1)
        }
    };
    (Some(directory), name)
}

pub(super) fn directory_path(directory: ProjectDirectory) -> RelativePath {
    RelativePath::new(directory_name(directory))
        .expect("the closed starter inventory uses valid relative paths")
}

fn directory_name(directory: ProjectDirectory) -> &'static str {
    match directory {
        ProjectDirectory::Assets => "assets",
        ProjectDirectory::Input => "input",
        ProjectDirectory::Prefabs => "prefabs",
        ProjectDirectory::Scenes => "scenes",
        ProjectDirectory::Schema => "schema",
        ProjectDirectory::Source => "src",
        ProjectDirectory::SourceBinaries => "src/bin",
    }
}

fn leaf_name(path: &RelativePath) -> &RelativeComponent {
    path.components()
        .last()
        .expect("the closed starter inventory has a non-empty relative path")
}

fn component(name: &str) -> RelativeComponent {
    RelativeComponent::new(name).expect("the closed starter inventory uses valid components")
}

pub(super) fn next_staging_name() -> Result<RelativeComponent, getrandom::Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut entropy = [0_u8; 16];
    getrandom::fill(&mut entropy)?;
    let mut suffix = String::with_capacity(entropy.len() * 2);
    for byte in entropy {
        suffix.push(char::from(HEX[usize::from(byte >> 4)]));
        suffix.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(component(&format!(".nara-stage-{suffix}")))
}
