//! Exact Workspace ownership transfer at the completed-file boundary.

use nara_tooling::{
    EditorPersistenceAuthority, EditorWorkspace, EditorWorkspaceCommandReport,
    EditorWorkspaceExtractionPersistenceCommit,
};

use super::*;

mod publication;
pub(crate) use publication::ExtractionPersistenceDestination;
pub(in crate::project_host::authoring::project::prefab_extraction) use publication::{
    InstalledPrefabExtractionProject, PrefabExtractionInstallationError,
    PrefabExtractionInstallationFailure, RetainedPrefabExtractionInstallation,
};

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PreparedPrefabExtractionWorkspaceInstallation
{
    project: BoundPrefabExtractionProject,
    persistence: PreparedEditorWorkspaceExtractionPersistence,
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct BoundPrefabExtractionWorkspaceInstallation<
    'w,
> {
    project: BoundPrefabExtractionProject,
    workspace: EditorWorkspaceExtractionPersistenceCommit<'w>,
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PrefabExtractionWorkspaceInstallationFailure
{
    prepared: PreparedPrefabExtractionWorkspaceInstallation,
    report: EditorWorkspaceCommandReport,
}

impl ProjectPrefabExtractionPublication {
    fn split_live_workspace(
        self,
    ) -> Result<
        (Self, PreparedEditorWorkspaceExtractionPersistence),
        (Self, ProjectPrefabExtractionPublicationError),
    > {
        if self.phase() != ProjectPrefabExtractionPublicationPhase::CheckpointsComplete
            || self.cleanup_pending()
            || self.uncertain()
        {
            return Err((self, ProjectPrefabExtractionPublicationError::Phase));
        }
        let Self {
            origin,
            entries,
            initialized,
            position,
            checkpoint_position,
            active,
            active_bytes,
            uncertain,
            #[cfg(test)]
            fail_next_write,
        } = self;
        let restore = |origin| Self {
            origin,
            entries,
            initialized,
            position,
            checkpoint_position,
            active,
            active_bytes,
            uncertain,
            #[cfg(test)]
            fail_next_write,
        };
        match origin {
            Origin::Live(storage) => match storage.into_installation() {
                Ok((installation, persistence)) => {
                    Ok((restore(Origin::LiveInstallation(installation)), persistence))
                }
                Err(failure) => {
                    let (storage, error) = failure.into_parts();
                    Err((
                        restore(Origin::Live(storage)),
                        ProjectPrefabExtractionPublicationError::Installation(error),
                    ))
                }
            },
            other => Err((
                restore(other),
                ProjectPrefabExtractionPublicationError::Phase,
            )),
        }
    }
}

impl BoundPrefabExtractionProject {
    /// Moves the exact original stage once. The remaining publisher no longer reaches through
    /// Workspace persistence for its nominal basis, physical validation or cumulative budget.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_workspace_installation(
        mut self,
    ) -> Result<
        PreparedPrefabExtractionWorkspaceInstallation,
        PrefabExtractionProjectPublicationRefusal,
    > {
        if let Err(error) = self.project.validate() {
            return Err(PrefabExtractionProjectPublicationRefusal { bound: self, error });
        }
        let Self { project, publisher } = self;
        let PreparedPrefabExtractionProject {
            future,
            session,
            ready,
            continuing,
        } = project;
        let ProjectPrefabExtractionFutureInventory {
            publication,
            inventory,
        } = future;
        let restore = |publication| Self {
            project: PreparedPrefabExtractionProject {
                future: ProjectPrefabExtractionFutureInventory {
                    publication,
                    inventory,
                },
                session,
                ready,
                continuing,
            },
            publisher,
        };
        match publication.split_live_workspace() {
            Ok((publication, persistence)) => Ok(PreparedPrefabExtractionWorkspaceInstallation {
                project: restore(publication),
                persistence,
            }),
            Err((publication, error)) => Err(PrefabExtractionProjectPublicationRefusal {
                bound: restore(publication),
                error: error.into(),
            }),
        }
    }
}

impl PreparedPrefabExtractionWorkspaceInstallation {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn project(
        &self,
    ) -> &BoundPrefabExtractionProject {
        &self.project
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn persistence(
        &self,
    ) -> &PreparedEditorWorkspaceExtractionPersistence {
        &self.persistence
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn project_mut(
        &mut self,
    ) -> &mut BoundPrefabExtractionProject {
        &mut self.project
    }

    /// The guard borrows only live Workspace. It owns the original prepared stage and can
    /// coexist with every native/lane guard and the separately owned final publisher.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn bind_workspace<'w>(
        self,
        authority: &EditorPersistenceAuthority,
        workspace: &'w mut EditorWorkspace,
    ) -> Result<
        BoundPrefabExtractionWorkspaceInstallation<'w>,
        PrefabExtractionWorkspaceInstallationFailure,
    > {
        let Self {
            project,
            persistence,
        } = self;
        match authority.bind_prefab_extraction_persistence(workspace, persistence) {
            Ok(workspace) => Ok(BoundPrefabExtractionWorkspaceInstallation { project, workspace }),
            Err(failure) => {
                let (persistence, report) = failure.into_parts();
                Err(PrefabExtractionWorkspaceInstallationFailure {
                    prepared: Self {
                        project,
                        persistence,
                    },
                    report,
                })
            }
        }
    }
}

impl BoundPrefabExtractionWorkspaceInstallation<'_> {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn project_mut(
        &mut self,
    ) -> &mut BoundPrefabExtractionProject {
        &mut self.project
    }

    /// No recapture, allocation, checkpoint conversion or filesystem work runs on restoration.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_prepared(
        self,
    ) -> PreparedPrefabExtractionWorkspaceInstallation {
        PreparedPrefabExtractionWorkspaceInstallation {
            project: self.project,
            persistence: self.workspace.into_prepared(),
        }
    }
}

impl PrefabExtractionWorkspaceInstallationFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (
        PreparedPrefabExtractionWorkspaceInstallation,
        EditorWorkspaceCommandReport,
    ) {
        (self.prepared, self.report)
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn report(
        &self,
    ) -> &EditorWorkspaceCommandReport {
        &self.report
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_prepared(
        self,
    ) -> PreparedPrefabExtractionWorkspaceInstallation {
        self.prepared
    }
}

impl fmt::Debug for PreparedPrefabExtractionWorkspaceInstallation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedPrefabExtractionWorkspaceInstallation")
            .field("persistence", &self.persistence)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for PrefabExtractionWorkspaceInstallationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionWorkspaceInstallationFailure")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}
