//! Current source/receipt selections shared by preview and source-owned operations.

use std::sync::Arc;

use nara_scene::{
    AuthoringSchemaBasis, SceneAuthoringRevision, SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::EditorDocumentId;

use crate::project_host::authoring::{
    AuthoringPrefabCaptureRequest, AuthoringPrefabSessionSnapshot, AuthoringPrefabSourceCapture,
    AuthoringProjectSourceAdmissionError, AuthoringSceneSessionSnapshot,
    AuthoringSourceCaptureInputs, PreparedAuthoringSchemaOwner,
    document::AuthoringDocumentSourceFact,
};
use crate::project_host::persistence::{FileSourceObservation, editor_digest};

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

    pub(super) fn has_confirmed_checkpoint(&self, editor: &EditorProjectSession) -> bool {
        if editor.persistence.has_pending_document(self.document) {
            return false;
        }
        let Some(receipt) = self.receipt.as_ref() else {
            return false;
        };
        let saved = editor
            .workspace
            .scene(self.document)
            .and_then(|slot| slot.saved_digest())
            .or_else(|| {
                editor
                    .workspace
                    .prefab(self.document)
                    .and_then(|slot| slot.saved_digest())
            });
        saved == Some(editor_digest(receipt.digest()))
    }
}

pub(super) struct SelectedPrefabSources {
    root: AuthoringSourceSelection,
    prefabs: Vec<AuthoringSourceSelection>,
}

#[derive(PartialEq, Eq)]
pub(super) struct SelectedPrefabBasis {
    root: AuthoringSourceSelection,
    participants: Vec<PrefabParticipant>,
}

#[derive(PartialEq, Eq)]
struct PrefabParticipant {
    key: SourceDocumentKey,
    project_path: Arc<str>,
    selected: Option<AuthoringSourceSelection>,
}

pub(super) fn prefab_participant_retention(key: &SourceDocumentKey, path: &str) -> Option<usize> {
    size_of::<PrefabParticipant>()
        .checked_add(key.retained_bytes())?
        .checked_add(path.len())?
        .checked_add(4 * size_of::<usize>())
}

impl SelectedPrefabSources {
    /// Retains only demanded sources, joined to the original selection of file or working copy.
    pub(super) fn complete(
        self,
        capture: &AuthoringPrefabSourceCapture,
    ) -> Option<SelectedPrefabBasis> {
        let mut participants = Vec::new();
        for prefab in capture.prefabs() {
            let key = prefab.native_source().key();
            let path = prefab.source().project_path().as_str();
            let mut matches = self
                .prefabs
                .iter()
                .filter(|source| &source.source_key == key || source.project_path.as_ref() == path);
            let selected = matches.next();
            if matches.next().is_some()
                || selected.map(|source| source.revision) != prefab.session_revision()
                || selected.is_some_and(|source| source.receipt.is_none())
            {
                return None;
            }
            participants.push(PrefabParticipant {
                key: key.clone(),
                project_path: Arc::from(path),
                selected: selected.cloned(),
            });
        }
        Some(SelectedPrefabBasis {
            root: self.root,
            participants,
        })
    }
}

impl SelectedPrefabBasis {
    /// Reports new selection facts after matching the root and every shared participant.
    pub(super) fn candidate_adds_participants(&self, candidate: &Self) -> Option<bool> {
        if self.root != candidate.root {
            return None;
        }
        let mut adds_participants = false;
        for observed in &candidate.participants {
            match self
                .participants
                .binary_search_by(|original| original.project_path.cmp(&observed.project_path))
            {
                Ok(index) if &self.participants[index] != observed => return None,
                Ok(_) => {}
                Err(_) => adds_participants = true,
            }
        }
        Some(adds_participants)
    }

    pub(super) fn checked_retained_bytes(&self) -> Option<usize> {
        let root = size_of::<Self>().checked_add(self.root.checked_retained_bytes()?)?;
        self.participants
            .iter()
            .try_fold(root, |bytes, participant| {
                bytes
                    .checked_add(prefab_participant_retention(
                        &participant.key,
                        &participant.project_path,
                    )?)?
                    .checked_add(
                        participant
                            .selected
                            .as_ref()
                            .map_or(Some(0), AuthoringSourceSelection::checked_retained_bytes)?,
                    )
            })
    }

    pub(super) fn matches(&self, editor: &EditorProjectSession) -> bool {
        self.matches_with_replacement(editor, None)
    }

    pub(super) fn used_working_copy(&self, expected: &AuthoringSourceSelection) -> bool {
        self.participants
            .iter()
            .any(|participant| participant.selected.as_ref() == Some(expected))
    }

    pub(super) fn apply_inputs_are_saved(
        &self,
        editor: &EditorProjectSession,
        destination: EditorDocumentId,
    ) -> bool {
        self.apply_inputs_are_saved_except(editor, &[destination])
    }

    pub(super) fn apply_inputs_are_saved_except(
        &self,
        editor: &EditorProjectSession,
        replacements: &[EditorDocumentId],
    ) -> bool {
        std::iter::once(&self.root)
            .chain(
                self.participants
                    .iter()
                    .filter_map(|participant| participant.selected.as_ref()),
            )
            .filter(|source| !replacements.contains(&source.document))
            .all(|source| {
                editor.workspace.document_is_dirty(source.document) == Some(false)
                    && source.has_confirmed_checkpoint(editor)
            })
    }

    pub(super) fn matches_after_prefab_apply(
        &self,
        editor: &EditorProjectSession,
        original: &AuthoringSourceSelection,
    ) -> bool {
        self.matches_with_replacement(editor, Some(original))
    }

    fn matches_with_replacement(
        &self,
        editor: &EditorProjectSession,
        replaced: Option<&AuthoringSourceSelection>,
    ) -> bool {
        editor.authoring_source_selection_is_current(&self.root)
            && self.participants.iter().all(|participant| {
                let selected = participant.selected.as_ref();
                replaced.is_some_and(|original| selected == Some(original))
                    || editor.authoring_dependency_selection_is_current(
                        &participant.key,
                        &participant.project_path,
                        selected,
                    )
            })
    }
}

impl EditorProjectSession {
    pub(super) fn prefab_source_capture_selection_bytes(
        &self,
        root: EditorDocumentId,
    ) -> Option<usize> {
        let root_bytes = self.authoring_source_selection_retained_bytes(root)?;
        self.prefab_lanes
            .keys()
            .try_fold(root_bytes, |bytes, &document| {
                bytes.checked_add(self.authoring_source_selection_retained_bytes(document)?)
            })?
            .checked_add(size_of::<SelectedPrefabSources>())?
            .checked_mul(2)
    }

    pub(super) fn capture_selected_prefab_sources(
        &self,
        root: AuthoringSourceSelection,
        kind: SourceDocumentKind,
        mut inputs: AuthoringSourceCaptureInputs,
    ) -> Result<
        (SelectedPrefabSources, AuthoringPrefabCaptureRequest),
        AuthoringProjectSourceAdmissionError,
    > {
        let receipt = root
            .receipt
            .as_ref()
            .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?;
        let schema = root
            .schema
            .as_ref()
            .ok_or(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch)?;
        let prefabs = self.authoring_prefab_selections();
        let observation = self.capture_authoring_source_observation(&root, &mut inputs)?;
        let snapshots =
            self.capture_open_prefabs(&prefabs, &mut inputs, Some((&root, &observation)))?;
        let request = self
            .authoring_project()
            .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?
            .capture_selected_prefab_request(
                receipt,
                observation,
                schema,
                kind,
                inputs,
                snapshots,
            )?;
        Ok((SelectedPrefabSources { root, prefabs }, request))
    }

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

    /// Acquires source custody only after its complete cost fits the capture allowance.
    pub(super) fn capture_authoring_source_observation(
        &self,
        selected: &AuthoringSourceSelection,
        inputs: &mut AuthoringSourceCaptureInputs,
    ) -> Result<FileSourceObservation, AuthoringProjectSourceAdmissionError> {
        let mismatch = || AuthoringProjectSourceAdmissionError::DocumentSourceMismatch;
        let source = selected.receipt.as_ref().ok_or_else(mismatch)?;
        if source.relative_path() != selected.project_path.as_ref()
            || source.source_key() != &selected.source_key
        {
            return Err(mismatch());
        }
        let expected = (source.identity(), source.parent_identity(), source.digest());
        match selected.source_key.kind() {
            nara_scene::SourceDocumentKind::Scene => {
                let lane = self
                    .scene_lanes
                    .get(&selected.document)
                    .ok_or_else(mismatch)?;
                if selected.project_path != lane.project_path
                    || selected.source_key != lane.source_key
                {
                    return Err(mismatch());
                }
                let mut guard = lane.host.try_bind().ok_or_else(mismatch)?;
                let host = guard.host_mut();
                let cost = host.source_observation_cost().ok_or_else(mismatch)?;
                inputs.capture_source_observation(cost, || {
                    host.capture_source_observation(expected)
                        .map_err(|_| mismatch())
                })
            }
            nara_scene::SourceDocumentKind::Prefab => {
                let lane = self
                    .prefab_lanes
                    .get(&selected.document)
                    .ok_or_else(mismatch)?;
                if selected.project_path != lane.project_path
                    || selected.source_key != lane.source_key
                {
                    return Err(mismatch());
                }
                let mut guard = lane.host.try_bind().ok_or_else(mismatch)?;
                let host = guard.host_mut();
                let cost = host.source_observation_cost().ok_or_else(mismatch)?;
                inputs.capture_source_observation(cost, || {
                    host.capture_source_observation(expected)
                        .map_err(|_| mismatch())
                })
            }
        }
    }

    /// Freezes each open source under the schema operation's existing cumulative allowance.
    pub(super) fn capture_authoring_schema_source_observation(
        &self,
        selected: &AuthoringSourceSelection,
        owner: &mut PreparedAuthoringSchemaOwner,
    ) -> Result<FileSourceObservation, AuthoringProjectSourceAdmissionError> {
        let mismatch = || AuthoringProjectSourceAdmissionError::DocumentSourceMismatch;
        let source = selected.receipt.as_ref().ok_or_else(mismatch)?;
        if source.relative_path() != selected.project_path.as_ref()
            || source.source_key() != &selected.source_key
        {
            return Err(mismatch());
        }
        let expected = (source.identity(), source.parent_identity(), source.digest());
        match selected.source_key.kind() {
            nara_scene::SourceDocumentKind::Scene => {
                let lane = self
                    .scene_lanes
                    .get(&selected.document)
                    .ok_or_else(mismatch)?;
                if selected.project_path != lane.project_path
                    || selected.source_key != lane.source_key
                {
                    return Err(mismatch());
                }
                let mut guard = lane.host.try_bind().ok_or_else(mismatch)?;
                let host = guard.host_mut();
                let cost = host.source_observation_cost().ok_or_else(mismatch)?;
                owner.capture_source_observation(cost, || {
                    host.capture_source_observation(expected)
                        .map_err(|_| mismatch())
                })
            }
            nara_scene::SourceDocumentKind::Prefab => {
                let lane = self
                    .prefab_lanes
                    .get(&selected.document)
                    .ok_or_else(mismatch)?;
                if selected.project_path != lane.project_path
                    || selected.source_key != lane.source_key
                {
                    return Err(mismatch());
                }
                let mut guard = lane.host.try_bind().ok_or_else(mismatch)?;
                let host = guard.host_mut();
                let cost = host.source_observation_cost().ok_or_else(mismatch)?;
                owner.capture_source_observation(cost, || {
                    host.capture_source_observation(expected)
                        .map_err(|_| mismatch())
                })
            }
        }
    }

    pub(super) fn capture_open_scenes(
        &self,
        selections: &[AuthoringSourceSelection],
        inputs: &mut AuthoringSourceCaptureInputs,
    ) -> Result<Box<[AuthoringSceneSessionSnapshot]>, AuthoringProjectSourceAdmissionError> {
        selections
            .iter()
            .map(|selected| {
                let session = self
                    .workspace
                    .scene(selected.document)
                    .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?
                    .session();
                let unavailable = || {
                    AuthoringSceneSessionSnapshot::unavailable(
                        selected.source_key.clone(),
                        Arc::clone(&selected.project_path),
                        session,
                    )
                };
                let Some(source) = selected.receipt.as_ref() else {
                    return Ok(unavailable());
                };
                let observation = match self.capture_authoring_source_observation(selected, inputs)
                {
                    Ok(observation) => observation,
                    Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch) => {
                        return Ok(unavailable());
                    }
                    Err(error) => return Err(error),
                };
                Ok(AuthoringSceneSessionSnapshot::capture(
                    source,
                    observation,
                    session,
                ))
            })
            .collect()
    }

    /// Selected requests share the root observation and defer all other Prefab observations.
    /// Full preview captures its observations here on the owner thread.
    pub(super) fn capture_open_prefabs(
        &self,
        selections: &[AuthoringSourceSelection],
        inputs: &mut AuthoringSourceCaptureInputs,
        selected_root: Option<(&AuthoringSourceSelection, &FileSourceObservation)>,
    ) -> Result<Box<[AuthoringPrefabSessionSnapshot]>, AuthoringProjectSourceAdmissionError> {
        selections
            .iter()
            .map(|selected| {
                let session = self
                    .workspace
                    .prefab(selected.document)
                    .ok_or(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)?
                    .session();
                let unavailable = || {
                    AuthoringPrefabSessionSnapshot::unavailable(
                        selected.source_key.clone(),
                        Arc::clone(&selected.project_path),
                        session,
                    )
                };
                let Some(source) = selected.receipt.as_ref() else {
                    return Ok(unavailable());
                };
                let observation = match selected_root {
                    Some((root, observation)) if root == selected => observation.clone(),
                    Some(_) => {
                        let Some(lane) = self.prefab_lanes.get(&selected.document) else {
                            return Ok(unavailable());
                        };
                        if selected.source_key.kind() != SourceDocumentKind::Prefab
                            || source.source_key() != &selected.source_key
                            || source.relative_path() != selected.project_path.as_ref()
                            || selected.source_key != lane.source_key
                            || selected.project_path != lane.project_path
                        {
                            return Ok(unavailable());
                        }
                        let host = lane.host.clone();
                        let expected =
                            (source.identity(), source.parent_identity(), source.digest());
                        return AuthoringPrefabSessionSnapshot::capture_deferred(
                            source,
                            session,
                            inputs,
                            move |admission| {
                                let mismatch =
                                    || AuthoringProjectSourceAdmissionError::DocumentSourceMismatch;
                                host.with_worker(|host| {
                                    let cost =
                                        host.source_observation_cost().ok_or_else(mismatch)?;
                                    admission.capture_source_observation(cost, || {
                                        host.capture_source_observation(expected)
                                            .map_err(|_| mismatch())
                                    })
                                })
                                .ok_or_else(mismatch)?
                            },
                        );
                    }
                    None => match self.capture_authoring_source_observation(selected, inputs) {
                        Ok(observation) => observation,
                        Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch) => {
                            return Ok(unavailable());
                        }
                        Err(error) => return Err(error),
                    },
                };
                Ok(AuthoringPrefabSessionSnapshot::capture(
                    source,
                    observation,
                    session,
                ))
            })
            .collect()
    }
}
