use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use nara::{
    fs::{DirectoryCapability, PathValidationError, RelativeComponent, RelativePath},
    project_workflow::{
        LocalDependencyRelation, NaraDependencySource, NaraDependencySourceError,
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
    #[error(
        "local creation requires a shared filesystem ancestor; cross-volume roots are unsupported"
    )]
    NoCommonAncestor,
    #[error("the local checkout relation is not a valid capability-relative path")]
    InvalidLocalRelation(#[source] PathValidationError),
    #[error("the shared local ancestor cannot authorize checkout and project-parent access")]
    CommonAncestorUnavailable(#[source] HostDirectoryError),
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
    let ancestor_path = checkout_path
        .ancestors()
        .find(|ancestor| parent_path.starts_with(ancestor))
        .ok_or(NewCommandError::NoCommonAncestor)?;
    let checkout_relation = endpoint_coordinate(&checkout_path, ancestor_path)?;
    let parent_relation = endpoint_coordinate(&parent_path, ancestor_path)?;
    let ancestor = host_fs::writable_directory(ancestor_path)
        .map_err(NewCommandError::CommonAncestorUnavailable)?;
    let publication_parent = parent_relation
        .as_ref()
        .map_or_else(
            || Ok(ancestor.share()),
            |path| {
                ancestor
                    .open_directory(path)
                    .map_err(HostDirectoryError::Capability)
            },
        )
        .map_err(NewCommandError::CommonAncestorUnavailable)?;
    let dependency = NaraDependencySource::local(LocalDependencyRelation::new(
        ancestor,
        checkout_relation,
        parent_relation,
    ));
    Ok((publication_parent, dependency))
}

fn endpoint_coordinate(
    path: &Path,
    ancestor: &Path,
) -> Result<Option<RelativePath>, NewCommandError> {
    let relative = path
        .strip_prefix(ancestor)
        .map_err(|_| NewCommandError::NoCommonAncestor)?;
    if relative.as_os_str().is_empty() {
        Ok(None)
    } else {
        logical_relative_path(relative)
            .map(Some)
            .map_err(NewCommandError::InvalidLocalRelation)
    }
}

fn logical_relative_path(path: &Path) -> Result<RelativePath, PathValidationError> {
    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => RelativeComponent::new(value),
            Component::CurDir => Err(PathValidationError::CurrentDirectory),
            Component::ParentDir => Err(PathValidationError::ParentTraversal),
            Component::RootDir | Component::Prefix(_) => {
                Err(PathValidationError::AbsoluteOrPrefixed)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    RelativePath::from_components(components)
}

fn resolve_parent(parent: PathBuf) -> Result<DirectoryCapability, NewCommandError> {
    let parent = fs::canonicalize(parent).map_err(NewCommandError::ResolveParent)?;
    host_fs::writable_directory(&parent).map_err(NewCommandError::from)
}
