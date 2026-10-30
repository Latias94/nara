//! Current source/receipt selections shared by preview and source-owned operations.

use std::sync::Arc;

use nara_scene::{AuthoringSchemaBasis, SceneAuthoringRevision, SourceDocumentKey};
use nara_tooling::EditorDocumentId;

use crate::project_host::authoring::{
    AuthoringPrefabSessionSnapshot, AuthoringProjectSourceAdmissionError,
    AuthoringSceneSessionSnapshot, document::AuthoringDocumentSourceFact,
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

impl AuthoringSourceSelection {
    pub(super) fn checked_retained_bytes(&self) -> Option<usize> {
        std::mem::size_of::<Self>()
            .checked_add(self.project_path.len())?
            .checked_add(self.source_key.retained_bytes())?
            .checked_add(
                self.receipt
                    .as_ref()
                    .map_or(Some(0), |source| source.checked_retained_bytes())?,
            )
    }
}

impl EditorProjectSession {
    pub(super) fn authoring_source_selection_is_current(
        &self,
        expected: &AuthoringSourceSelection,
    ) -> bool {
        let (revision, schema, key, path, source, observation) =
            if let Some(slot) = self.workspace.scene(expected.document) {
                let Some(lane) = self.scene_lanes.get(&expected.document) else {
                    return false;
                };
                (
                    slot.revision(),
                    slot.session().authoring_schema_basis(),
                    &lane.source_key,
                    lane.project_path.as_ref(),
                    lane.authoring_source.as_ref(),
                    lane.host
                        .try_with(|host| host.accepted_source_observation())
                        .flatten(),
                )
            } else if let Some(slot) = self.workspace.prefab(expected.document) {
                let Some(lane) = self.prefab_lanes.get(&expected.document) else {
                    return false;
                };
                (
                    slot.revision(),
                    slot.session().authoring_schema_basis(),
                    &lane.source_key,
                    lane.project_path.as_ref(),
                    lane.authoring_source.as_ref(),
                    lane.host
                        .try_with(|host| host.accepted_source_observation())
                        .flatten(),
                )
            } else {
                return false;
            };
        if revision != expected.revision
            || schema != expected.schema
            || key != &expected.source_key
            || path != expected.project_path.as_ref()
        {
            return false;
        }
        match (source, observation, expected.receipt.as_ref()) {
            (Some(base), Some((identity, parent, digest)), Some(receipt)) => {
                receipt.matches_persistence_observation(base, identity, parent, digest)
            }
            (None, _, None) | (_, None, None) => true,
            _ => false,
        }
    }

    pub(super) fn authoring_dependency_selection_is_current(
        &self,
        key: &SourceDocumentKey,
        path: &str,
        expected: Option<&AuthoringSourceSelection>,
    ) -> bool {
        let mut current = self
            .scene_lanes
            .iter()
            .map(|(&document, lane)| (document, &lane.source_key, lane.project_path.as_ref()))
            .chain(
                self.prefab_lanes.iter().map(|(&document, lane)| {
                    (document, &lane.source_key, lane.project_path.as_ref())
                }),
            )
            .filter(|(_, current_key, current_path)| *current_key == key || *current_path == path);
        let selected = current.next();
        if current.next().is_some() {
            return false;
        }
        match (selected, expected) {
            (None, None) => true,
            (Some((document, _, _)), Some(expected)) => {
                document == expected.document
                    && self.authoring_source_selection_is_current(expected)
            }
            _ => false,
        }
    }

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
            .checked_add(
                source
                    .as_ref()
                    .map_or(Some(0), |source| source.checked_retained_bytes())?,
            )
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
        let mut selections = Vec::with_capacity(self.prefab_lanes.len());
        selections.extend(
            self.prefab_lanes
                .keys()
                .filter_map(|&document| self.authoring_source_selection(document)),
        );
        selections
    }

    pub(super) fn authoring_scene_selections(
        &self,
        root: EditorDocumentId,
    ) -> Vec<AuthoringSourceSelection> {
        let count = self.scene_lanes.len() - usize::from(self.scene_lanes.contains_key(&root));
        let mut selections = Vec::with_capacity(count);
        selections.extend(
            self.scene_lanes
                .keys()
                .filter(|&&document| document != root)
                .filter_map(|&document| self.authoring_source_selection(document)),
        );
        selections
    }

    pub(super) fn scene_working_set_retained_bytes(
        &self,
        root: EditorDocumentId,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        let mut bytes = 0_usize;
        for (&document, lane) in &self.scene_lanes {
            if document == root {
                continue;
            }
            let Some(slot) = self.workspace.scene(document) else {
                continue;
            };
            let retained = match lane.authoring_source.as_ref() {
                Some(source) => {
                    AuthoringSceneSessionSnapshot::capture_retained_bytes(source, slot.session())?
                }
                None => AuthoringSceneSessionSnapshot::unavailable_retained_bytes(
                    &lane.source_key,
                    &lane.project_path,
                    slot.session(),
                )?,
            };
            bytes = bytes
                .checked_add(retained)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        }
        bytes
            .checked_add(self.prefab_working_set_retained_bytes()?)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    }

    pub(super) fn prefab_working_set_retained_bytes(
        &self,
    ) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        let mut bytes = 0_usize;
        for (&document, lane) in &self.prefab_lanes {
            let Some(slot) = self.workspace.prefab(document) else {
                continue;
            };
            let retained = match lane.authoring_source.as_ref() {
                Some(source) => {
                    AuthoringPrefabSessionSnapshot::capture_retained_bytes(source, slot.session())?
                }
                None => AuthoringPrefabSessionSnapshot::unavailable_retained_bytes(
                    &lane.source_key,
                    &lane.project_path,
                    slot.session(),
                )?,
            };
            bytes = bytes
                .checked_add(retained)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        }
        Ok(bytes)
    }

    pub(super) fn capture_open_scenes(
        &self,
        selections: &[AuthoringSourceSelection],
    ) -> Box<[AuthoringSceneSessionSnapshot]> {
        selections
            .iter()
            .filter_map(|selected| {
                let lane = self.scene_lanes.get(&selected.document)?;
                let session = self.workspace.scene(selected.document)?.session();
                let Some(source) = selected.receipt.as_ref().or(lane.authoring_source.as_ref())
                else {
                    return Some(AuthoringSceneSessionSnapshot::unavailable(
                        selected.source_key.clone(),
                        Arc::clone(&selected.project_path),
                        session,
                    ));
                };
                let snapshot = AuthoringSceneSessionSnapshot::capture(source, session);
                Some(if selected.receipt.is_some() {
                    snapshot
                } else {
                    snapshot.without_persistence()
                })
            })
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
                let session = self.workspace.prefab(selected.document)?.session();
                let Some(source) = selected.receipt.as_ref().or(lane.authoring_source.as_ref())
                else {
                    return Some(AuthoringPrefabSessionSnapshot::unavailable(
                        selected.source_key.clone(),
                        Arc::clone(&selected.project_path),
                        session,
                    ));
                };
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
