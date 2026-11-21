use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryPublicationReceipt, FileCapability, FsError,
};

use super::{
    ProjectCreationAdmissionError, ProjectCreationAdmissionErrorInner, ProjectCreationOutcome,
    ProjectCreationReceipt, PublishedButUnadmitted,
    stage::{directory_path, member_path},
};
use crate::{
    project_host::{
        ProjectContentMemberKind, StartupContentClosureReceipt,
        check::{ProjectSourceCheckError, check_project_source_with_expected_digests},
    },
    project_workflow::{
        dependency::PreparedDependencyRevalidation,
        starter::{
            ProjectCreationPlanReceipt, ProjectMember, STARTER_ACTION_MAP_PATH,
            desktop_2d_product_recipe,
        },
    },
};

pub(super) fn admit_published(
    publication_parent: &DirectoryCapability,
    publication: DirectoryPublicationReceipt,
    plan: ProjectCreationPlanReceipt,
    dependency_revalidation: PreparedDependencyRevalidation,
) -> ProjectCreationOutcome {
    let project_root = match publication_parent.open_child_directory(publication.destination()) {
        Ok(project_root) => project_root,
        Err(error) => {
            return ProjectCreationOutcome::PublishedButUnadmitted(PublishedButUnadmitted {
                plan,
                publication,
                published_root: None,
                error: ProjectCreationAdmissionError {
                    inner: ProjectCreationAdmissionErrorInner::ReopenPublishedDirectory(error),
                },
            });
        }
    };
    if project_root.identity() != publication.published_identity() {
        let observed = project_root.identity();
        let expected = publication.published_identity();
        return ProjectCreationOutcome::PublishedButUnadmitted(PublishedButUnadmitted {
            plan,
            publication,
            published_root: None,
            error: ProjectCreationAdmissionError {
                inner: ProjectCreationAdmissionErrorInner::PublishedIdentityMismatch {
                    expected,
                    observed,
                },
            },
        });
    }
    if let Err(error) = dependency_revalidation.revalidate(publication_parent) {
        return ProjectCreationOutcome::PublishedButUnadmitted(PublishedButUnadmitted {
            plan,
            publication,
            published_root: Some(project_root),
            error: ProjectCreationAdmissionError {
                inner: ProjectCreationAdmissionErrorInner::DependencyRelation(error),
            },
        });
    }

    match admit_project(&project_root, &plan) {
        Ok(admission) => ProjectCreationOutcome::Created(ProjectCreationReceipt {
            plan,
            publication,
            startup_content_closure: admission,
        }),
        Err(error) => ProjectCreationOutcome::PublishedButUnadmitted(PublishedButUnadmitted {
            plan,
            publication,
            published_root: Some(project_root),
            error,
        }),
    }
}

fn admit_project(
    project_root: &DirectoryCapability,
    plan: &ProjectCreationPlanReceipt,
) -> Result<StartupContentClosureReceipt, ProjectCreationAdmissionError> {
    for directory in plan.directories() {
        project_root
            .open_directory(&directory_path(*directory))
            .map_err(|source| ProjectCreationAdmissionError {
                inner: ProjectCreationAdmissionErrorInner::PublishedDirectory {
                    directory: *directory,
                    source,
                },
            })?;
    }
    for receipt in plan.members() {
        let member = receipt.member();
        if !matches!(
            member,
            ProjectMember::ProjectManifest
                | ProjectMember::StartupScene
                | ProjectMember::InputActionMap
        ) {
            verified_member(project_root, member, receipt.digest())?;
        }
    }
    let manifest_digest = plan
        .member(ProjectMember::ProjectManifest)
        .expect("the closed starter receipt contains the project manifest")
        .digest();
    let startup_scene_digest = plan
        .member(ProjectMember::StartupScene)
        .expect("the closed starter receipt contains the startup Scene")
        .digest();
    let recipe = desktop_2d_product_recipe(plan.starter()).map_err(|error| {
        ProjectCreationAdmissionError {
            inner: ProjectCreationAdmissionErrorInner::RecipeConstruction(Box::new(error)),
        }
    })?;
    let checked = check_project_source_with_expected_digests(
        project_root.share(),
        None,
        recipe,
        manifest_digest,
        startup_scene_digest,
    )
    .map_err(project_admission_error)?;
    let (_, content) = checked.into_parts();
    let closure = content
        .startup_content_closure()
        .expect("project admission loads the manifest's startup Scene");
    if let Some(expected) = plan.member(ProjectMember::InputActionMap) {
        let selected = closure.members().iter().find(|member| {
            member.kind() == ProjectContentMemberKind::InputActionMap
                && member.path().as_str() == STARTER_ACTION_MAP_PATH
        });
        let error = match selected {
            Some(member) if member.digest() == expected.digest() => None,
            Some(member) => Some(FsError::DigestMismatch {
                expected: expected.digest(),
                actual: member.digest(),
            }),
            None => Some(FsError::TargetStateMismatch),
        };
        if let Some(source) = error {
            return Err(ProjectCreationAdmissionError {
                inner: ProjectCreationAdmissionErrorInner::PublishedMember {
                    member: ProjectMember::InputActionMap,
                    source,
                },
            });
        }
    }
    Ok(closure)
}

fn project_admission_error(error: ProjectSourceCheckError) -> ProjectCreationAdmissionError {
    let inner = match error {
        ProjectSourceCheckError::Manifest(error) => {
            ProjectCreationAdmissionErrorInner::Manifest(Box::new(error))
        }
        ProjectSourceCheckError::ProductComposition(error) => {
            ProjectCreationAdmissionErrorInner::ProductRecipe(Box::new(error))
        }
        ProjectSourceCheckError::StartupContent { error, .. } => {
            ProjectCreationAdmissionErrorInner::Content(Box::new(error))
        }
    };
    ProjectCreationAdmissionError { inner }
}

fn verified_member(
    root: &DirectoryCapability,
    member: ProjectMember,
    digest: ContentDigest,
) -> Result<FileCapability, ProjectCreationAdmissionError> {
    let file = open_member(root, member)?;
    file.verify_digest(digest, digest.length())
        .map_err(|source| ProjectCreationAdmissionError {
            inner: ProjectCreationAdmissionErrorInner::PublishedMember { member, source },
        })?;
    Ok(file)
}

fn open_member(
    root: &DirectoryCapability,
    member: ProjectMember,
) -> Result<FileCapability, ProjectCreationAdmissionError> {
    root.open_file(&member_path(member))
        .map_err(|source| ProjectCreationAdmissionError {
            inner: ProjectCreationAdmissionErrorInner::PublishedMember { member, source },
        })
}
