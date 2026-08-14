use std::{fs, io, path::PathBuf};

use nara::{
    fs::{DirectoryCapability, PathValidationError, RelativePath},
    project_workflow::{
        LocalDependencyParent, NaraDependencySource, NaraDependencySourceError,
        ProjectCreationOutcome, ProjectCreationPlanError, ProjectCreationRequest, ProjectSlug,
        ProjectSlugError, prepare_desktop_2d_project,
    },
};
use thiserror::Error;

use super::{
    args::{DependencyArguments, NewArguments},
    host_fs::{self, HostDirectoryError},
};

#[derive(Debug, Error)]
pub(crate) enum NewCommandError {
    #[error("failed to resolve the Nara checkout directory")]
    ResolveCheckout(#[source] io::Error),
    #[error("failed to resolve the project parent directory")]
    ResolveParent(#[source] io::Error),
    #[error("the project parent is outside the selected Nara checkout")]
    ParentOutsideCheckout,
    #[error("the project parent relation is not a valid capability-relative path")]
    InvalidParentRelation(#[source] PathValidationError),
    #[error(transparent)]
    HostDirectory(#[from] HostDirectoryError),
    #[error(transparent)]
    ProjectSlug(#[from] ProjectSlugError),
    #[error(transparent)]
    DependencySource(#[from] NaraDependencySourceError),
    #[error(transparent)]
    ProjectPlan(#[from] ProjectCreationPlanError),
}

pub(crate) fn execute(arguments: NewArguments) -> Result<ProjectCreationOutcome, NewCommandError> {
    let slug = ProjectSlug::new(arguments.slug)?;
    let (publication_parent, dependency) = match arguments.dependency {
        DependencyArguments::LocalCheckout { checkout } => {
            prepare_local_dependency(checkout, arguments.parent)?
        }
        DependencyArguments::ImmutableGit {
            repository,
            revision,
        } => (
            resolve_parent(arguments.parent)?,
            NaraDependencySource::immutable_git(repository, revision)?,
        ),
    };
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        publication_parent,
        slug,
        dependency,
    ))?;
    Ok(prepared.publish())
}

fn prepare_local_dependency(
    checkout: PathBuf,
    parent: PathBuf,
) -> Result<(DirectoryCapability, NaraDependencySource), NewCommandError> {
    let checkout_path = fs::canonicalize(checkout).map_err(NewCommandError::ResolveCheckout)?;
    let parent_path = fs::canonicalize(parent).map_err(NewCommandError::ResolveParent)?;
    let relative_parent = parent_path
        .strip_prefix(&checkout_path)
        .map_err(|_| NewCommandError::ParentOutsideCheckout)?;
    let checkout = host_fs::writable_directory(&checkout_path)?;
    let (publication_parent, relation) = if relative_parent.as_os_str().is_empty() {
        (checkout.share(), LocalDependencyParent::CheckoutRoot)
    } else {
        let relative_parent =
            RelativePath::new(relative_parent).map_err(NewCommandError::InvalidParentRelation)?;
        let publication_parent = checkout
            .open_directory(&relative_parent)
            .map_err(HostDirectoryError::Capability)?;
        (
            publication_parent,
            LocalDependencyParent::Descendant(relative_parent),
        )
    };
    let dependency = NaraDependencySource::local(checkout, relation);
    Ok((publication_parent, dependency))
}

fn resolve_parent(parent: PathBuf) -> Result<DirectoryCapability, NewCommandError> {
    let parent = fs::canonicalize(parent).map_err(NewCommandError::ResolveParent)?;
    host_fs::writable_directory(&parent).map_err(NewCommandError::from)
}
