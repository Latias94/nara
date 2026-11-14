//! Source dependency interest and one queued successor belong to the original Scene owner.

use nara_app::RuntimeGeneration;
use nara_asset::AssetSourceChange;
use nara_identity::{SceneInstanceId, SpawnedSceneInstance, WorldIdentityDomainId};

use crate::project_content::{ProjectContentClosureReceipt, ProjectSourceObservations};

use super::{ProjectSceneRequestError, ProjectSceneRequestId, ProjectSceneSourceUpdateStatus};

pub(super) struct SourceUpdates {
    published: ProjectContentClosureReceipt,
    generation: RuntimeGeneration,
    instance: (WorldIdentityDomainId, SceneInstanceId),
    retry: Option<ProjectSourceObservations>,
    status: ProjectSceneSourceUpdateStatus,
}

impl SourceUpdates {
    pub(super) fn new(
        published: ProjectContentClosureReceipt,
        generation: RuntimeGeneration,
        instance: &SpawnedSceneInstance,
    ) -> Self {
        Self {
            published,
            generation,
            instance: (instance.domain_id(), instance.instance_id()),
            retry: None,
            status: ProjectSceneSourceUpdateStatus {
                request: None,
                pending: false,
                error: None,
            },
        }
    }

    pub(super) fn status(&self) -> ProjectSceneSourceUpdateStatus {
        self.status
    }

    pub(super) fn selected_scene(&self) -> &nara_project::ProjectPath {
        self.published.selected_scene_path()
    }

    pub(super) fn matches_current(
        &self,
        generation: RuntimeGeneration,
        instance: &SpawnedSceneInstance,
    ) -> bool {
        self.generation == generation
            && self.instance == (instance.domain_id(), instance.instance_id())
    }

    pub(super) fn notify(&mut self, change: &AssetSourceChange, automatic_in_flight: bool) -> bool {
        let matches = self
            .published
            .members()
            .iter()
            .any(|member| matches_path(change, member.path().as_str()))
            || self.retry.as_ref().is_some_and(|retry| {
                retry
                    .paths()
                    .iter()
                    .any(|path| matches_path(change, path.as_str()))
            });
        if matches || automatic_in_flight {
            self.rescan();
            true
        } else {
            false
        }
    }

    pub(super) fn rescan(&mut self) {
        self.status.pending = true;
    }

    pub(super) fn started(&mut self, request: ProjectSceneRequestId) {
        self.status.request = Some(request);
        self.status.pending = false;
        self.status.error = None;
    }

    pub(super) fn refused(&mut self, error: ProjectSceneRequestError) {
        self.status.error = Some(error);
    }

    pub(super) fn failed(
        &mut self,
        request: ProjectSceneRequestId,
        error: ProjectSceneRequestError,
        observations: Option<ProjectSourceObservations>,
    ) {
        if self.status.request != Some(request) {
            return;
        }
        self.status.error = Some(error);
        if let Some(observations) = observations
            && observations.is_complete()
            && observations.project_root_identity() == self.published.project_root_identity()
        {
            self.retry = Some(observations);
        }
    }

    pub(super) fn publish(
        &mut self,
        published: ProjectContentClosureReceipt,
        instance: &SpawnedSceneInstance,
    ) {
        self.published = published;
        self.instance = (instance.domain_id(), instance.instance_id());
        self.retry = None;
        self.status.error = None;
    }

    pub(super) fn acknowledge(&mut self, request: ProjectSceneRequestId) {
        if self.status.request == Some(request) {
            self.status.request = None;
        }
    }
}

fn matches_path(change: &AssetSourceChange, path: &str) -> bool {
    (change.raw_source_changed() && path == change.path().as_str())
        || (change.raw_metadata_changed()
            && (path == change.path().as_str()
                || path.strip_suffix(".meta") == Some(change.path().as_str())))
}
