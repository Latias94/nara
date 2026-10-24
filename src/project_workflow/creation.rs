use nara_asset::StableAssetId;
use nara_core::SourceUniverseId;
use nara_fs::{DirectoryCapability, RelativeComponent};
use thiserror::Error;

use super::{
    dependency::{
        DependencyPreparationError, NaraDependencySource, PreparedDependency,
        PreparedDependencyRevalidation, prepare_dependency,
    },
    starter::{ProjectCreationPlan, ProjectSlug, build_desktop_2d_v1_plan},
};

pub struct ProjectCreationRequest {
    publication_parent: DirectoryCapability,
    slug: ProjectSlug,
    dependency: NaraDependencySource,
}

impl ProjectCreationRequest {
    #[must_use]
    pub fn new(
        publication_parent: DirectoryCapability,
        slug: ProjectSlug,
        dependency: NaraDependencySource,
    ) -> Self {
        Self {
            publication_parent,
            slug,
            dependency,
        }
    }
}

pub struct PreparedProjectCreation {
    pub(crate) publication_parent: DirectoryCapability,
    pub(crate) destination: RelativeComponent,
    pub(crate) plan: ProjectCreationPlan,
    pub(crate) dependency_revalidation: PreparedDependencyRevalidation,
}

impl PreparedProjectCreation {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlan {
        &self.plan
    }

    #[must_use = "the project-creation terminal outcome must be handled"]
    pub fn publish(self) -> super::publication::ProjectCreationOutcome {
        super::publication::publish_prepared(self)
    }
}

impl std::fmt::Debug for PreparedProjectCreation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedProjectCreation")
            .field("parent_identity", &self.publication_parent.identity())
            .field("destination", &self.destination)
            .field("plan", &self.plan)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectCreationPlanErrorKind {
    LocalCheckoutUnavailable,
    LocalParentUnavailable,
    LocalParentMismatch,
    NonUtf8LocalCoordinate,
    SourceIdentityEntropy,
    StarterContract,
}

#[derive(Debug, Error)]
pub enum ProjectCreationPlanError {
    #[error("local dependency checkout could not be resolved through the common ancestor")]
    LocalCheckoutUnavailable(#[source] nara_fs::FsError),
    #[error("local dependency destination parent could not be resolved")]
    LocalParentUnavailable(#[source] nara_fs::FsError),
    #[error("local dependency destination parent does not match the common-ancestor relation")]
    LocalParentMismatch,
    #[error("local dependency coordinate is not UTF-8 and cannot be represented in Cargo.toml")]
    NonUtf8LocalCoordinate,
    #[error("project source identity could not be generated")]
    SourceIdentityEntropy(#[source] getrandom::Error),
    #[error("desktop-2d/v1 starter contract is invalid: {0}")]
    StarterContract(&'static str),
}

impl ProjectCreationPlanError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCreationPlanErrorKind {
        match self {
            Self::LocalCheckoutUnavailable(_) => {
                ProjectCreationPlanErrorKind::LocalCheckoutUnavailable
            }
            Self::LocalParentUnavailable(_) => ProjectCreationPlanErrorKind::LocalParentUnavailable,
            Self::LocalParentMismatch => ProjectCreationPlanErrorKind::LocalParentMismatch,
            Self::NonUtf8LocalCoordinate => ProjectCreationPlanErrorKind::NonUtf8LocalCoordinate,
            Self::SourceIdentityEntropy(_) => ProjectCreationPlanErrorKind::SourceIdentityEntropy,
            Self::StarterContract(_) => ProjectCreationPlanErrorKind::StarterContract,
        }
    }
}

pub fn prepare_desktop_2d_project(
    request: ProjectCreationRequest,
) -> Result<PreparedProjectCreation, ProjectCreationPlanError> {
    let ProjectCreationRequest {
        publication_parent,
        slug,
        dependency,
    } = request;
    let destination = slug.destination_component();
    let dependency =
        prepare_dependency(dependency, &publication_parent).map_err(|error| match error {
            DependencyPreparationError::LocalCheckoutUnavailable(error) => {
                ProjectCreationPlanError::LocalCheckoutUnavailable(error)
            }
            DependencyPreparationError::LocalParentUnavailable(error) => {
                ProjectCreationPlanError::LocalParentUnavailable(error)
            }
            DependencyPreparationError::LocalParentMismatch => {
                ProjectCreationPlanError::LocalParentMismatch
            }
            DependencyPreparationError::NonUtf8LocalCoordinate => {
                ProjectCreationPlanError::NonUtf8LocalCoordinate
            }
        })?;
    let PreparedDependency {
        manifest_fragment,
        creation_command_arguments,
        receipt,
        revalidation: dependency_revalidation,
    } = dependency;
    let (source_universe, startup_scene_identity) = prepare_source_identities(getrandom::fill)?;
    let plan = build_desktop_2d_v1_plan(
        slug,
        source_universe,
        startup_scene_identity,
        &manifest_fragment,
        &creation_command_arguments,
        receipt,
    )
    .map_err(ProjectCreationPlanError::StarterContract)?;
    Ok(PreparedProjectCreation {
        publication_parent,
        destination,
        plan,
        dependency_revalidation,
    })
}

fn prepare_source_identities(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<(SourceUniverseId, StableAssetId), ProjectCreationPlanError> {
    let mut entropy = [[0_u8; 16]; 2];
    fill(entropy.as_flattened_mut()).map_err(ProjectCreationPlanError::SourceIdentityEntropy)?;
    let [project, scene] = entropy;
    let project = uuid::Builder::from_random_bytes(project).into_uuid();
    let scene = uuid::Builder::from_random_bytes(scene).into_uuid();
    Ok((
        SourceUniverseId::try_from_uuid(project).expect("a version-4 UUID is not nil"),
        StableAssetId::from_uuid(scene),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_entropy_is_consumed_once_and_failure_is_explicit() {
        let (universe, scene) = prepare_source_identities(|bytes| {
            assert_eq!(bytes.len(), 32);
            bytes[..16].fill(1);
            bytes[16..].fill(2);
            Ok(())
        })
        .unwrap();
        assert_eq!(universe.to_string(), "01010101-0101-4101-8101-010101010101");
        assert_eq!(scene.to_string(), "02020202-0202-4202-8202-020202020202");
        let error = prepare_source_identities(|_| Err(getrandom::Error::UNSUPPORTED)).unwrap_err();
        assert_eq!(
            error.kind(),
            ProjectCreationPlanErrorKind::SourceIdentityEntropy
        );
    }
}
