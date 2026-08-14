use nara_fs::{ContentDigest, DirectoryCapability, DirectoryPublicationReceipt, FileCapability};
use nara_reflect::SchemaCompositionFingerprint;

use super::{
    ProjectCreationAdmissionError, ProjectCreationAdmissionErrorInner, ProjectCreationOutcome,
    ProjectCreationReceipt, PublishedButUnadmitted,
    stage::{directory_path, member_path},
};
use crate::{
    ProductRecipe,
    project_host::{
        ProjectContentLoader, ProjectContentRevision, ProjectSettingsLineage,
        ingest_project_manifest_with_expected_digest, resolve_product_recipe,
    },
    project_workflow::{
        dependency::PreparedDependencyRevalidation,
        starter::{ProjectCreationPlanReceipt, ProjectMember},
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
            project_root,
            settings_lineage: admission.settings_lineage,
            schema_fingerprint: admission.schema_fingerprint,
            content_revision: admission.content_revision,
            content_digest: admission.content_digest,
        }),
        Err(error) => ProjectCreationOutcome::PublishedButUnadmitted(PublishedButUnadmitted {
            plan,
            publication,
            published_root: Some(project_root),
            error,
        }),
    }
}

struct ProjectAdmissionFacts {
    settings_lineage: ProjectSettingsLineage,
    schema_fingerprint: SchemaCompositionFingerprint,
    content_revision: ProjectContentRevision,
    content_digest: ContentDigest,
}

fn admit_project(
    project_root: &DirectoryCapability,
    plan: &ProjectCreationPlanReceipt,
) -> Result<ProjectAdmissionFacts, ProjectCreationAdmissionError> {
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
    let manifest_receipt = plan
        .member(ProjectMember::ProjectManifest)
        .expect("the closed starter receipt contains the project manifest");
    let manifest = open_member(project_root, ProjectMember::ProjectManifest)?;
    let candidate =
        ingest_project_manifest_with_expected_digest(&manifest, None, manifest_receipt.digest())
            .map_err(|error| ProjectCreationAdmissionError {
                inner: ProjectCreationAdmissionErrorInner::Manifest(Box::new(error)),
            })?;
    let runtime_plan =
        resolve_product_recipe(&candidate, ProductRecipe::new()).map_err(|error| {
            ProjectCreationAdmissionError {
                inner: ProjectCreationAdmissionErrorInner::ProductRecipe(Box::new(error)),
            }
        })?;
    let startup_scene_digest = plan
        .member(ProjectMember::StartupScene)
        .expect("the closed starter receipt contains the startup Scene")
        .digest();
    let content = ProjectContentLoader::new(project_root.share())
        .and_then(|loader| {
            loader.load_with_expected_startup_scene_digest(
                &candidate,
                &runtime_plan,
                startup_scene_digest,
            )
        })
        .map_err(|error| ProjectCreationAdmissionError {
            inner: ProjectCreationAdmissionErrorInner::Content(Box::new(error)),
        })?;
    debug_assert_eq!(content.lineage(), candidate.lineage());
    Ok(ProjectAdmissionFacts {
        settings_lineage: candidate.lineage(),
        schema_fingerprint: content.schema_fingerprint(),
        content_revision: content.revision(),
        content_digest: content.content_digest(),
    })
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
