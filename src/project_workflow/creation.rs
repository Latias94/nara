use nara_asset::StableAssetId;
use nara_core::SourceUniverseId;
use nara_fs::{DirectoryCapability, RelativeComponent};
use thiserror::Error;

use super::{
    dependency::{
        DependencyPreparationError, NaraDependencySource, PreparedDependency,
        PreparedDependencyRevalidation, prepare_dependency,
    },
    starter::{ProjectCreationPlan, ProjectSlug, ProjectStarter, build_creation_plan},
};

pub struct ProjectCreationRequest {
    starter: ProjectStarter,
    publication_parent: DirectoryCapability,
    slug: ProjectSlug,
    dependency: NaraDependencySource,
}

impl ProjectCreationRequest {
    /// Selects the exact starter version, destination authority and Nara dependency.
    #[must_use]
    pub fn new(
        starter: ProjectStarter,
        publication_parent: DirectoryCapability,
        slug: ProjectSlug,
        dependency: NaraDependencySource,
    ) -> Self {
        Self {
            starter,
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
    #[error("generated starter contract is invalid: {0}")]
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
        starter,
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
    let identities = prepare_source_identities(starter, getrandom::fill)?;
    let plan = build_creation_plan(
        starter,
        slug,
        identities,
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

pub(super) struct StarterSourceIdentities {
    pub(super) universe: SourceUniverseId,
    pub(super) scene: StableAssetId,
    pub(super) action_map: Option<StableAssetId>,
}

fn prepare_source_identities(
    starter: ProjectStarter,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<StarterSourceIdentities, ProjectCreationPlanError> {
    let mut entropy = [[0_u8; 16]; 3];
    let identity_count = 2 + usize::from(starter.has_action_map());
    fill(entropy[..identity_count].as_flattened_mut())
        .map_err(ProjectCreationPlanError::SourceIdentityEntropy)?;
    let [project, scene, action_map] = entropy;
    let project = uuid::Builder::from_random_bytes(project).into_uuid();
    let asset_id =
        |bytes| StableAssetId::from_uuid(uuid::Builder::from_random_bytes(bytes).into_uuid());
    Ok(StarterSourceIdentities {
        universe: SourceUniverseId::try_from_uuid(project).expect("a version-4 UUID is not nil"),
        scene: asset_id(scene),
        action_map: starter.has_action_map().then(|| asset_id(action_map)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_entropy_is_consumed_once_and_failure_is_explicit() {
        let identities = prepare_source_identities(ProjectStarter::Desktop2dV1, |bytes| {
            assert_eq!(bytes.len(), 32);
            bytes[..16].fill(1);
            bytes[16..].fill(2);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            identities.universe.to_string(),
            "01010101-0101-4101-8101-010101010101"
        );
        assert_eq!(
            identities.scene.to_string(),
            "02020202-0202-4202-8202-020202020202"
        );
        assert!(identities.action_map.is_none());
        let error = prepare_source_identities(ProjectStarter::Desktop2dV2, |bytes| {
            assert_eq!(bytes.len(), 48);
            Err(getrandom::Error::UNSUPPORTED)
        })
        .err()
        .unwrap();
        assert_eq!(
            error.kind(),
            ProjectCreationPlanErrorKind::SourceIdentityEntropy
        );
    }
}
