use nara_fs::{ContentDigest, DirectoryCapability, FsError, RelativePath};
use nara_project::ProjectPath;

use crate::{
    ProductRecipe,
    project_host::{ProjectContentDeliveryReceipt, ProjectSceneContentStamp},
    project_workflow::ProjectCheckError,
};

use super::{ProjectExportBinaryTarget, check_delivery, stage::ProjectExportStageError};

#[derive(Debug, thiserror::Error)]
pub(super) enum ProjectExportStagingError {
    #[error(transparent)]
    Tree(#[from] ProjectExportStageError),
    #[error("failed to open the staged project directory")]
    OpenProject(#[source] FsError),
    #[error("the staged project failed its {target:?} delivery check")]
    Delivery {
        target: ProjectExportBinaryTarget,
        #[source]
        source: Box<ProjectCheckError>,
    },
    #[error("the staged {target:?} delivery differs from the checked source delivery")]
    DeliveryMismatch { target: ProjectExportBinaryTarget },
}

#[cfg(feature = "project-tooling")]
impl ProjectExportStagingError {
    pub(super) fn delivery_failure(
        &self,
    ) -> Option<(ProjectExportBinaryTarget, Option<&ProjectCheckError>)> {
        match self {
            Self::Delivery { target, source } => Some((*target, Some(source))),
            Self::DeliveryMismatch { target } => Some((*target, None)),
            _ => None,
        }
    }
}

pub(super) struct StagedDeliveryChecks {
    recipe: ProductRecipe,
    expected: [Option<DeliveryObservation>; 2],
}

#[derive(PartialEq, Eq)]
struct DeliveryObservation {
    delivery_digest: ContentDigest,
    startup: ProjectSceneContentStamp,
}

impl From<&ProjectContentDeliveryReceipt> for DeliveryObservation {
    fn from(delivery: &ProjectContentDeliveryReceipt) -> Self {
        Self {
            delivery_digest: delivery.delivery_digest(),
            startup: delivery.startup_content().content_closure().stamp(),
        }
    }
}

impl StagedDeliveryChecks {
    pub(super) fn new(
        deliveries: &[ProjectContentDeliveryReceipt; 2],
        recipe: ProductRecipe,
    ) -> Self {
        let [headless, desktop] = deliveries;
        let expected = [(headless, desktop), (desktop, headless)].map(|(selected, other)| {
            adds_visible_members(selected, other).then(|| DeliveryObservation::from(selected))
        });
        Self { recipe, expected }
    }

    pub(super) fn verify(
        &self,
        staging: DirectoryCapability,
    ) -> Result<(), ProjectExportStagingError> {
        if self.expected.iter().all(Option::is_none) {
            return Ok(());
        }
        let path = RelativePath::new("project").expect("the export project directory is fixed");
        let project = staging
            .open_directory_exact(&path)
            .map_err(ProjectExportStagingError::OpenProject)?;
        for (target, expected) in ProjectExportBinaryTarget::ALL
            .into_iter()
            .zip(&self.expected)
        {
            let Some(expected) = expected else {
                continue;
            };
            let actual =
                check_delivery(project.share(), target, self.recipe.clone()).map_err(|source| {
                    ProjectExportStagingError::Delivery {
                        target,
                        source: Box::new(source),
                    }
                })?;
            if &DeliveryObservation::from(&actual) != expected {
                return Err(ProjectExportStagingError::DeliveryMismatch { target });
            }
        }
        Ok(())
    }
}

fn adds_visible_members(
    selected: &ProjectContentDeliveryReceipt,
    other: &ProjectContentDeliveryReceipt,
) -> bool {
    let directories = selected.runtime_directories();
    let is_descendant = |path: &ProjectPath| {
        directories.iter().any(|root| {
            path.as_str()
                .strip_prefix(root.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'))
        })
    };
    other.members().iter().any(|member| {
        is_descendant(member.path())
            && selected
                .members()
                .binary_search_by(|original| original.path().as_str().cmp(member.path().as_str()))
                .is_err()
    }) || other
        .runtime_directories()
        .iter()
        .any(|directory| is_descendant(directory) && !directories.contains(directory))
}
