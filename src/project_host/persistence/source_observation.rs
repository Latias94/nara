//! Read-only source custody handed to finite authoring work under the original Host guard.

use super::*;

impl ScenePersistenceHost {
    pub(in crate::project_host) fn source_observation_cost(
        &self,
    ) -> Option<FileSourceObservationCost> {
        self.file.publication.source_observation_cost()
    }

    pub(in crate::project_host) fn capture_source_observation(
        &self,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
    ) -> Result<FileSourceObservation, EditorPersistenceRejection> {
        self.file
            .publication
            .capture_source_observation(expected)
            .map_err(editor_publication_rejection)
    }
}

impl PrefabPersistenceHost {
    pub(in crate::project_host) fn source_observation_cost(
        &self,
    ) -> Option<FileSourceObservationCost> {
        self.file.publication.source_observation_cost()
    }

    pub(in crate::project_host) fn capture_source_observation(
        &self,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
    ) -> Result<FileSourceObservation, EditorPersistenceRejection> {
        self.file
            .publication
            .capture_source_observation(expected)
            .map_err(editor_publication_rejection)
    }
}
