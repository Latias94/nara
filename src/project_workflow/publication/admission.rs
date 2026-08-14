use nara_fs::{ContentDigest, DirectoryCapability, DirectoryPublicationReceipt, FileCapability};

use super::{
    ProjectCreationAdmissionError, ProjectCreationAdmissionErrorInner, ProjectCreationOutcome,
    ProjectCreationReceipt, PublishedButUnadmitted,
    stage::{directory_path, member_path},
};
use crate::{
    project_host::{
        StartupContentClosureReceipt,
        check::{ProjectSourceCheckError, check_project_source_with_expected_digests},
    },
    project_workflow::{
        dependency::PreparedDependencyRevalidation,
        starter::{ProjectCreationPlanReceipt, ProjectMember, desktop_2d_v1_product_recipe},
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
            ProjectMember::ProjectManifest | ProjectMember::StartupScene
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
    let checked = check_project_source_with_expected_digests(
        project_root.share(),
        None,
        desktop_2d_v1_product_recipe(),
        manifest_digest,
        startup_scene_digest,
    )
    .map_err(project_admission_error)?;
    let (_, content) = checked.into_parts();
    Ok(content.startup_content_closure())
}

fn project_admission_error(error: ProjectSourceCheckError) -> ProjectCreationAdmissionError {
    let inner = match error {
        ProjectSourceCheckError::Manifest(error) => {
            ProjectCreationAdmissionErrorInner::Manifest(Box::new(error))
        }
        ProjectSourceCheckError::ProductComposition(error) => {
            ProjectCreationAdmissionErrorInner::ProductRecipe(Box::new(error))
        }
        ProjectSourceCheckError::StartupContent(error) => {
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
