use std::io::{self, Write};

use nara_fs::{DirectoryCapability, FsError, FsOperation, RelativeComponent, RelativePath};

use super::super::starter::{
    ProjectCreationPlan, ProjectDirectory, ProjectMember, ProjectMemberPlan,
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
    let assets = create_directory(&staging, ProjectDirectory::Assets, "assets")?;
    let prefabs = create_directory(&staging, ProjectDirectory::Prefabs, "prefabs")?;
    let scenes = create_directory(&staging, ProjectDirectory::Scenes, "scenes")?;
    let schema = create_directory(&staging, ProjectDirectory::Schema, "schema")?;
    let source = create_directory(&staging, ProjectDirectory::Source, "src")?;
    let source_binaries = create_directory(&source, ProjectDirectory::SourceBinaries, "bin")?;
    let tree = StagedProjectTree {
        root: staging,
        assets,
        prefabs,
        scenes,
        schema,
        source,
        source_binaries,
    };

    for member in plan.members() {
        write_member(&tree, member)?;
    }
    for member in plan.members() {
        verify_staged_member(&tree.root, member)?;
    }
    sync_directory(
        &tree.source_binaries,
        Some(ProjectDirectory::SourceBinaries),
    )?;
    sync_directory(&tree.source, Some(ProjectDirectory::Source))?;
    sync_directory(&tree.assets, Some(ProjectDirectory::Assets))?;
    sync_directory(&tree.prefabs, Some(ProjectDirectory::Prefabs))?;
    sync_directory(&tree.scenes, Some(ProjectDirectory::Scenes))?;
    sync_directory(&tree.schema, Some(ProjectDirectory::Schema))?;
    sync_directory(&tree.root, None)?;
    Ok(())
}

struct StagedProjectTree {
    root: DirectoryCapability,
    assets: DirectoryCapability,
    prefabs: DirectoryCapability,
    scenes: DirectoryCapability,
    schema: DirectoryCapability,
    source: DirectoryCapability,
    source_binaries: DirectoryCapability,
}

fn create_directory(
    parent: &DirectoryCapability,
    directory: ProjectDirectory,
    name: &str,
) -> Result<DirectoryCapability, ProjectTreeStageError> {
    parent
        .create_child_directory(&component(name))
        .map_err(|source| ProjectTreeStageError::CreateDirectory { directory, source })
}

fn write_member(
    tree: &StagedProjectTree,
    member: &ProjectMemberPlan,
) -> Result<(), ProjectTreeStageError> {
    let parent = match member.member() {
        ProjectMember::CargoManifest
        | ProjectMember::ProjectManifest
        | ProjectMember::AuthorGuide => &tree.root,
        ProjectMember::AuthoringSchemaBundle => &tree.schema,
        ProjectMember::LibrarySource => &tree.source,
        ProjectMember::HeadlessSource | ProjectMember::DesktopSource => &tree.source_binaries,
        ProjectMember::StartupScene | ProjectMember::StartupSceneMetadata => &tree.scenes,
    };
    let mut file = parent
        .create_child_file(&member_name(member.member()))
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
    RelativePath::new(match member {
        ProjectMember::CargoManifest => "Cargo.toml",
        ProjectMember::ProjectManifest => "nara.toml",
        ProjectMember::AuthoringSchemaBundle => "schema/authoring.bundle.json",
        ProjectMember::AuthorGuide => "README.md",
        ProjectMember::LibrarySource => "src/lib.rs",
        ProjectMember::HeadlessSource => "src/bin/headless.rs",
        ProjectMember::DesktopSource => "src/bin/desktop.rs",
        ProjectMember::StartupScene => "scenes/startup.scene.json",
        ProjectMember::StartupSceneMetadata => "scenes/startup.scene.json.meta",
    })
    .expect("the closed starter inventory uses valid relative paths")
}

pub(super) fn directory_path(directory: ProjectDirectory) -> RelativePath {
    RelativePath::new(match directory {
        ProjectDirectory::Assets => "assets",
        ProjectDirectory::Prefabs => "prefabs",
        ProjectDirectory::Scenes => "scenes",
        ProjectDirectory::Schema => "schema",
        ProjectDirectory::Source => "src",
        ProjectDirectory::SourceBinaries => "src/bin",
    })
    .expect("the closed starter inventory uses valid relative paths")
}

fn member_name(member: ProjectMember) -> RelativeComponent {
    member_path(member)
        .components()
        .last()
        .cloned()
        .expect("the closed starter inventory has non-empty relative paths")
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
