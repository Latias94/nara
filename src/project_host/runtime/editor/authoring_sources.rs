//! Current source/receipt selections shared by preview and source-owned operations.

use std::sync::Arc;

use nara_scene::{AuthoringSchemaBasis, SceneAuthoringRevision, SourceDocumentKey};
use nara_tooling::EditorDocumentId;

use crate::project_host::authoring::{
    AuthoringPrefabSessionSnapshot, document::AuthoringDocumentSourceFact,
};

use super::EditorProjectSession;

mod failure;
pub(super) use failure::authoring_source_failure;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct AuthoringSourceSelection {
    pub(super) document: EditorDocumentId,
    pub(super) revision: SceneAuthoringRevision,
    pub(super) schema: Option<AuthoringSchemaBasis>,
    pub(super) source_key: SourceDocumentKey,
    pub(super) project_path: Arc<str>,
    pub(super) receipt: Option<AuthoringDocumentSourceFact>,
}

impl EditorProjectSession {
    pub(super) fn authoring_source_selection_retained_bytes(
        &self,
        document: EditorDocumentId,
    ) -> Option<usize> {
        let (path, key, source) = if let Some(lane) = self.scene_lanes.get(&document) {
            (&lane.project_path, &lane.source_key, &lane.authoring_source)
        } else {
            let lane = self.prefab_lanes.get(&document)?;
            (&lane.project_path, &lane.source_key, &lane.authoring_source)
        };
        std::mem::size_of::<AuthoringSourceSelection>()
            .checked_add(path.len())?
            .checked_add(key.retained_bytes())?
            .checked_add(source.as_ref()?.checked_retained_bytes()?)
    }

    pub(super) fn authoring_source_selection(
        &self,
        document: EditorDocumentId,
    ) -> Option<AuthoringSourceSelection> {
        if let Some(slot) = self.workspace.scene(document) {
            let lane = self.scene_lanes.get(&document)?;
            let receipt = lane.authoring_source.as_ref().and_then(|source| {
                let (identity, parent, digest) = lane
                    .host
                    .try_with(|host| host.accepted_source_observation())??;
                source
                    .with_persistence_observation(identity, parent, digest)
                    .ok()
            });
            return Some(AuthoringSourceSelection {
                document,
                revision: slot.revision(),
                schema: slot.session().authoring_schema_basis(),
                source_key: lane.source_key.clone(),
                project_path: Arc::clone(&lane.project_path),
                receipt,
            });
        }
        let slot = self.workspace.prefab(document)?;
        let lane = self.prefab_lanes.get(&document)?;
        let receipt = lane.authoring_source.as_ref().and_then(|source| {
            let (identity, parent, digest) = lane
                .host
                .try_with(|host| host.accepted_source_observation())??;
            source
                .with_persistence_observation(identity, parent, digest)
                .ok()
        });
        Some(AuthoringSourceSelection {
            document,
            revision: slot.revision(),
            schema: slot.session().authoring_schema_basis(),
            source_key: lane.source_key.clone(),
            project_path: Arc::clone(&lane.project_path),
            receipt,
        })
    }

    pub(super) fn authoring_prefab_selections(&self) -> Vec<AuthoringSourceSelection> {
        self.prefab_lanes
            .keys()
            .filter_map(|&document| self.authoring_source_selection(document))
            .collect()
    }

    pub(super) fn capture_open_prefabs(
        &self,
        selections: &[AuthoringSourceSelection],
    ) -> Box<[AuthoringPrefabSessionSnapshot]> {
        selections
            .iter()
            .filter_map(|selected| {
                let lane = self.prefab_lanes.get(&selected.document)?;
                let source = selected
                    .receipt
                    .as_ref()
                    .or(lane.authoring_source.as_ref())?;
                let session = self.workspace.prefab(selected.document)?.session();
                let snapshot = AuthoringPrefabSessionSnapshot::capture(source, session);
                Some(if selected.receipt.is_some() {
                    snapshot
                } else {
                    snapshot.without_persistence()
                })
            })
            .collect()
    }
}
