//! Live Workspace custody and saved checkpoints for a verified component Apply pair.

use std::{mem::size_of, sync::Arc};

use nara_scene::{
    __private::{
        BoundPrefabComponentApply, CapturedPrefabComponentApplySources, PrefabComponentApplySource,
        PrefabComponentApplySourceMut, PreparedPrefabComponentApply,
        bind_prepared_prefab_component_apply, capture_prefab_component_apply_sources,
    },
    AuthoringDocumentReadmission, PrefabAuthoringSession, PrefabDocumentCandidate,
    SceneDocumentCandidate, SceneFileEncoding, SceneFileLimits, ScenePatchReport,
    SourceDocumentKey,
};

use super::{
    EditorDocumentDigest, EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis,
    EditorDocumentSlot, EditorDocumentSourceBinding, EditorPersistenceAuthority,
    EditorPersistenceCaptureIdentity, EditorPersistenceCheckpoint, EditorPersistenceCommit,
    EditorPersistenceContent, EditorPersistenceEncodingError, EditorPrefabSlot, EditorWorkspace,
    EditorWorkspaceCommandReport, EditorWorkspacePersistenceIdentity,
    EncodedEditorPersistenceCheckpoint, WorkspaceDocumentContext,
    authoring_query::rejected_basis_report,
    persistence_checkpoint::{BoundSavedFields, WorkspaceSavedInput},
    workspace_error_report,
};

#[derive(Debug)]
struct ComponentApplySlot {
    basis: EditorDocumentQueryBasis,
    kind: EditorDocumentKind,
    binding: EditorDocumentSourceBinding,
    saved: WorkspaceSavedInput,
}

/// The exact working and saved bases of the two existing files selected by the Host.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabComponentApplyInput {
    owner: Arc<EditorWorkspacePersistenceIdentity>,
    containing: ComponentApplySlot,
    destination: ComponentApplySlot,
    sources: CapturedPrefabComponentApplySources,
}

impl EditorPersistenceAuthority {
    /// Captures both named sources, including any unsaved work in the containing document.
    /// The Host supplies the canonical keys from the corresponding file bindings.
    #[doc(hidden)]
    pub fn capture_prefab_component_apply(
        &self,
        workspace: &EditorWorkspace,
        containing: &EditorDocumentQueryBasis,
        containing_key: &SourceDocumentKey,
        destination: &EditorDocumentQueryBasis,
        destination_key: &SourceDocumentKey,
        mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<EditorWorkspacePrefabComponentApplyInput, EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        if !Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            return Err(refused(
                active,
                containing.document,
                "Component Apply requires the original hosted Workspace authority",
            ));
        }
        if containing.document == destination.document {
            return Err(refused(
                active,
                destination.document,
                "Component Apply requires a distinct destination Prefab document",
            ));
        }
        let containing_slot = workspace
            .resolve_document_query_slot(containing, None)
            .map_err(|error| rejected_basis_report(workspace, containing, error))?;
        let destination_slot = workspace
            .resolve_document_query_slot(destination, Some(EditorDocumentKind::Prefab))
            .map_err(|error| rejected_basis_report(workspace, destination, error))?;
        let EditorDocumentSlot::Prefab(destination_source) = destination_slot else {
            return Err(refused(
                active,
                destination.document,
                "Component Apply destination must be an open Prefab",
            ));
        };
        let containing_source = match containing_slot {
            EditorDocumentSlot::Scene(slot) => PrefabComponentApplySource::Scene(&slot.session),
            EditorDocumentSlot::Prefab(slot) => PrefabComponentApplySource::Prefab(&slot.session),
            EditorDocumentSlot::Extension(_) => {
                return Err(refused(
                    active,
                    containing.document,
                    "Component Apply requires a containing Scene or Prefab",
                ));
            }
        };
        let storage = size_of::<(
            Arc<EditorWorkspacePersistenceIdentity>,
            [ComponentApplySlot; 2],
        )>();
        admit(Some(storage))
            .map_err(|report| source_report(active, containing.document, report))?;
        let containing = capture_slot(active, containing, containing_slot, &mut admit)?;
        let destination = capture_slot(active, destination, destination_slot, &mut admit)?;
        if containing.binding == destination.binding {
            return Err(refused(
                active,
                destination.basis.document,
                "Component Apply requires two distinct source bindings",
            ));
        }
        let sources = capture_prefab_component_apply_sources(
            containing_source,
            containing_key,
            &destination_source.session,
            destination_key,
            admit,
        )
        .map_err(|report| source_report(active, containing.basis.document, report))?;
        Ok(EditorWorkspacePrefabComponentApplyInput {
            owner: Arc::clone(&self.identity),
            containing,
            destination,
            sources,
        })
    }
}

fn capture_slot(
    active: Option<EditorDocumentId>,
    basis: &EditorDocumentQueryBasis,
    slot: &EditorDocumentSlot,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<ComponentApplySlot, EditorWorkspaceCommandReport> {
    let (Some(binding), Some(saved)) = (slot.source_binding(), WorkspaceSavedInput::capture(slot))
    else {
        return Err(refused(
            active,
            basis.document,
            "Component Apply requires a bound authoring source",
        ));
    };
    admit(Some(binding.locator().len()))
        .map_err(|report| source_report(active, basis.document, report))?;
    Ok(ComponentApplySlot {
        basis: basis.clone(),
        kind: slot.kind(),
        binding: binding.clone(),
        saved,
    })
}

impl EditorWorkspacePrefabComponentApplyInput {
    #[must_use]
    pub fn containing_basis(&self) -> &EditorDocumentQueryBasis {
        &self.containing.basis
    }

    #[must_use]
    pub fn destination_basis(&self) -> &EditorDocumentQueryBasis {
        &self.destination.basis
    }

    #[must_use]
    pub fn containing_binding(&self) -> &EditorDocumentSourceBinding {
        &self.containing.binding
    }

    #[must_use]
    pub fn destination_binding(&self) -> &EditorDocumentSourceBinding {
        &self.destination.binding
    }

    #[must_use]
    pub fn source_pair(&self) -> &CapturedPrefabComponentApplySources {
        &self.sources
    }

    pub fn validate(
        &self,
        workspace: &EditorWorkspace,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        let active = workspace.active_document;
        if !Arc::ptr_eq(&self.owner, &workspace.persistence_identity) {
            return Err(refused(
                active,
                self.containing.basis.document,
                "Component Apply belongs to a different Workspace",
            ));
        }
        for source in [&self.containing, &self.destination] {
            let slot = workspace
                .resolve_document_query_slot(&source.basis, Some(source.kind))
                .map_err(|error| rejected_basis_report(workspace, &source.basis, error))?;
            if slot.source_binding() != Some(&source.binding)
                || WorkspaceSavedInput::capture(slot) != Some(source.saved)
            {
                return Err(refused(
                    active,
                    source.basis.document,
                    "A Component Apply source binding, saved checkpoint or reload state changed",
                ));
            }
        }
        Ok(())
    }

    /// Creates both fixed canonical outputs in containing/destination order. The returned
    /// checkpoints travel independently to the existing file publication owner.
    pub fn prepare_persistence(
        self,
        scene_pair: PreparedPrefabComponentApply,
        mut admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<
        (
            PreparedEditorWorkspacePrefabComponentApply,
            [EncodedEditorPersistenceCheckpoint; 2],
        ),
        EditorWorkspacePrefabComponentApplyPrepareFailure,
    > {
        match self.prepare_outputs(&scene_pair, &mut admit) {
            Ok(
                [
                    (containing, containing_output),
                    (destination, destination_output),
                ],
            ) => Ok((
                PreparedEditorWorkspacePrefabComponentApply {
                    persistence: ComponentApplyPersistence {
                        input: self,
                        outputs: [containing, destination],
                    },
                    scene_pair,
                },
                [containing_output, destination_output],
            )),
            Err(report) => Err(EditorWorkspacePrefabComponentApplyPrepareFailure {
                input: self,
                scene_pair,
                report,
            }),
        }
    }

    fn prepare_outputs(
        &self,
        pair: &PreparedPrefabComponentApply,
        admit: &mut impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<
        [(ComponentApplyOutput, EncodedEditorPersistenceCheckpoint); 2],
        EditorWorkspaceCommandReport,
    > {
        let containing = self.containing.basis.document;
        let destination = self.destination.basis.document;
        if !self.sources.owns_prepared(pair) {
            return Err(refused(
                None,
                containing,
                "Component Apply preparation belongs to a different source capture",
            ));
        }
        let containing_encoding =
            require_file_encoding(containing, pair.containing_file_encoding())?;
        let destination_encoding =
            require_file_encoding(destination, pair.destination_file_encoding())?;
        let storage = size_of::<[(ComponentApplyOutput, EncodedEditorPersistenceCheckpoint); 2]>();
        admit(Some(storage)).map_err(|report| source_report(None, containing, report))?;
        let containing_content = match self.containing.kind {
            EditorDocumentKind::Scene => pair
                .shared_containing_scene()
                .map(EditorPersistenceContent::Scene),
            EditorDocumentKind::Prefab => pair
                .shared_containing_prefab()
                .map(EditorPersistenceContent::Prefab),
            EditorDocumentKind::Extension(_) => None,
        }
        .ok_or_else(|| {
            refused(
                None,
                containing,
                "The prepared containing source kind changed",
            )
        })?;
        pair.admit_containing_persistence_work(&mut *admit)
            .map_err(|report| source_report(None, containing, report))?;
        pair.admit_destination_persistence_work(&mut *admit)
            .map_err(|report| source_report(None, destination, report))?;
        let containing_readmission = pair
            .containing_readmission(&mut *admit)
            .map_err(|report| source_report(None, containing, report))?;
        let destination_readmission = pair
            .destination_readmission(&mut *admit)
            .map_err(|report| source_report(None, destination, report))?;
        let containing_checkpoint = EditorPersistenceAuthority::capture_shared(
            &self.owner,
            containing,
            self.containing.kind,
            Some(self.containing.binding.clone()),
            pair.containing_future_revision(),
            pair.containing_content_state(),
            containing_content,
        );
        let destination_checkpoint = EditorPersistenceAuthority::capture_shared(
            &self.owner,
            destination,
            EditorDocumentKind::Prefab,
            Some(self.destination.binding.clone()),
            pair.destination_future_revision(),
            pair.destination_content_state(),
            EditorPersistenceContent::Prefab(pair.shared_destination()),
        );
        Ok([
            encode_output(
                containing_checkpoint,
                containing_encoding,
                pair.containing_file_limits(),
                containing_readmission,
                admit,
            )?,
            encode_output(
                destination_checkpoint,
                destination_encoding,
                pair.destination_file_limits(),
                destination_readmission,
                admit,
            )?,
        ])
    }
}

fn require_file_encoding(
    document: EditorDocumentId,
    encoding: Option<SceneFileEncoding>,
) -> Result<SceneFileEncoding, EditorWorkspaceCommandReport> {
    encoding.ok_or_else(|| {
        refused(
            None,
            document,
            "Component Apply requires the captured source file encoding",
        )
    })
}

#[derive(Debug)]
struct ComponentApplyOutput {
    capture: Arc<EditorPersistenceCaptureIdentity>,
    digest: EditorDocumentDigest,
    file_limits: SceneFileLimits,
    readmission: AuthoringDocumentReadmission,
}

impl ComponentApplyOutput {
    fn accepts(&self, commit: &EditorPersistenceCommit) -> bool {
        Arc::ptr_eq(&self.capture, &commit.checkpoint.capture) && self.digest == commit.digest()
    }
}

fn encode_output(
    checkpoint: EditorPersistenceCheckpoint,
    encoding: SceneFileEncoding,
    limits: SceneFileLimits,
    readmission: AuthoringDocumentReadmission,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<(ComponentApplyOutput, EncodedEditorPersistenceCheckpoint), EditorWorkspaceCommandReport>
{
    let document = checkpoint.document();
    let encoded = checkpoint
        .encode_with_admission(encoding, limits, |bytes| admit(Some(bytes)))
        .map_err(|failure| match failure.into_parts().1 {
            EditorPersistenceEncodingError::Admission(report) => {
                source_report(None, document, report)
            }
            _ => refused(
                None,
                document,
                "Component Apply output failed canonical encoding",
            ),
        })?;
    let bytes = encoded.bytes();
    admit(Some(bytes.len())).map_err(|report| source_report(None, document, report))?;
    let decode_failure = |_| {
        refused(
            None,
            document,
            "Component Apply output failed file decoding",
        )
    };
    let preserved = match encoded.checkpoint().kind() {
        EditorDocumentKind::Scene => {
            let candidate = SceneDocumentCandidate::decode_bytes_with_admission(
                bytes,
                encoding,
                limits,
                |_| Ok::<(), std::convert::Infallible>(()),
            )
            .map_err(decode_failure)?;
            readmission.preserves_scene_candidate_with_admission(candidate, admit)
        }
        EditorDocumentKind::Prefab => {
            let candidate = PrefabDocumentCandidate::decode_bytes_with_admission(
                bytes,
                encoding,
                limits,
                |_| Ok::<(), std::convert::Infallible>(()),
            )
            .map_err(decode_failure)?;
            readmission.preserves_prefab_candidate_with_admission(candidate, admit)
        }
        EditorDocumentKind::Extension(_) => {
            return Err(refused(
                None,
                document,
                "Component Apply output must be a Scene or Prefab",
            ));
        }
    }
    .map_err(|report| source_report(None, document, report))?;
    if !preserved {
        return Err(refused(
            None,
            document,
            "Component Apply output changed the captured authoring classification",
        ));
    }
    let digest = encoded.digest();
    let output = ComponentApplyOutput {
        capture: Arc::clone(&encoded.checkpoint().capture),
        digest: EditorDocumentDigest::new(digest.length(), *digest.as_bytes()),
        file_limits: limits,
        readmission,
    };
    Ok((output, encoded))
}

#[derive(Debug)]
struct ComponentApplyPersistence {
    input: EditorWorkspacePrefabComponentApplyInput,
    outputs: [ComponentApplyOutput; 2],
}

/// A verified Scene pair together with the exact identities of its two encoded checkpoints.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PreparedEditorWorkspacePrefabComponentApply {
    persistence: ComponentApplyPersistence,
    scene_pair: PreparedPrefabComponentApply,
}

impl PreparedEditorWorkspacePrefabComponentApply {
    #[must_use]
    pub fn input(&self) -> &EditorWorkspacePrefabComponentApplyInput {
        &self.persistence.input
    }

    #[must_use]
    pub fn scene_pair(&self) -> &PreparedPrefabComponentApply {
        &self.scene_pair
    }

    #[must_use]
    pub fn authoring_readmission(
        &self,
        document: EditorDocumentId,
    ) -> Option<&AuthoringDocumentReadmission> {
        self.output(document).map(|output| &output.readmission)
    }

    #[must_use]
    pub fn file_limits(&self, document: EditorDocumentId) -> Option<SceneFileLimits> {
        self.output(document).map(|output| output.file_limits)
    }

    fn output(&self, document: EditorDocumentId) -> Option<&ComponentApplyOutput> {
        if document == self.input().containing_basis().document {
            Some(&self.persistence.outputs[0])
        } else if document == self.input().destination_basis().document {
            Some(&self.persistence.outputs[1])
        } else {
            None
        }
    }

    pub fn validate(
        &self,
        workspace: &EditorWorkspace,
    ) -> Result<(), EditorWorkspaceCommandReport> {
        self.input().validate(workspace)
    }
}

/// Preparation refusal returns the exact capture and verified Scene tokens.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabComponentApplyPrepareFailure {
    input: EditorWorkspacePrefabComponentApplyInput,
    scene_pair: PreparedPrefabComponentApply,
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspacePrefabComponentApplyPrepareFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        EditorWorkspacePrefabComponentApplyInput,
        PreparedPrefabComponentApply,
        EditorWorkspaceCommandReport,
    ) {
        (self.input, self.scene_pair, self.report)
    }
}

/// Bind refusal preserves the exact pair and both accepted publications.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabComponentApplyBindFailure {
    prepared: PreparedEditorWorkspacePrefabComponentApply,
    commits: [EditorPersistenceCommit; 2],
    report: EditorWorkspaceCommandReport,
}

impl EditorWorkspacePrefabComponentApplyBindFailure {
    pub fn report(&self) -> &EditorWorkspaceCommandReport {
        &self.report
    }

    pub fn into_parts(
        self,
    ) -> (
        PreparedEditorWorkspacePrefabComponentApply,
        [EditorPersistenceCommit; 2],
        EditorWorkspaceCommandReport,
    ) {
        (self.prepared, self.commits, self.report)
    }
}

/// Exclusive guard for the exact live source pair and its saved fields.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct EditorWorkspacePrefabComponentApplyCommit<'w> {
    persistence: ComponentApplyPersistence,
    scene_pair: BoundPrefabComponentApply<'w>,
    saved: [BoundSavedFields<'w>; 2],
    commits: [EditorPersistenceCommit; 2],
}

impl EditorWorkspacePrefabComponentApplyCommit<'_> {
    /// Releases the live borrows with the original paired preparation and publications intact.
    pub fn into_prepared(
        self,
    ) -> (
        PreparedEditorWorkspacePrefabComponentApply,
        [EditorPersistenceCommit; 2],
    ) {
        (
            PreparedEditorWorkspacePrefabComponentApply {
                persistence: self.persistence,
                scene_pair: self.scene_pair.into_prepared(),
            },
            self.commits,
        )
    }

    /// Installs both new source histories and acknowledges the accepted saves together.
    pub fn commit(self) {
        let [containing_commit, destination_commit] = self.commits;
        let [containing_saved, destination_saved] = self.saved;
        let (containing, destination) = self.scene_pair.commit();
        let containing_state = containing_commit.checkpoint.content_state();
        let containing_acknowledged = match containing {
            PrefabComponentApplySourceMut::Scene(session) => {
                session.acknowledge_source_saved(containing_state)
            }
            PrefabComponentApplySourceMut::Prefab(session) => {
                session.acknowledge_source_saved(containing_state)
            }
        };
        let destination_acknowledged =
            destination.acknowledge_source_saved(destination_commit.checkpoint.content_state());
        debug_assert!(
            containing_acknowledged && destination_acknowledged,
            "both saved content states were prebound to their exact future histories"
        );
        containing_saved.commit(&containing_commit);
        destination_saved.commit(&destination_commit);
    }
}

impl EditorPersistenceAuthority {
    /// Preflights both live slots and both exact accepted commits before binding the pair.
    /// Dropping the returned guard leaves the incumbent Workspace unchanged.
    #[doc(hidden)]
    pub fn bind_prefab_component_apply<'w>(
        &self,
        workspace: &'w mut EditorWorkspace,
        prepared: PreparedEditorWorkspacePrefabComponentApply,
        commits: [EditorPersistenceCommit; 2],
    ) -> Result<
        EditorWorkspacePrefabComponentApplyCommit<'w>,
        EditorWorkspacePrefabComponentApplyBindFailure,
    > {
        let active = workspace.active_document;
        let containing = prepared.input().containing_basis().document;
        let destination = prepared.input().destination_basis().document;
        let preflight = if Arc::ptr_eq(&self.identity, &workspace.persistence_identity) {
            prepared.validate(workspace).and_then(|()| {
                for ((output, commit), document) in prepared
                    .persistence
                    .outputs
                    .iter()
                    .zip(&commits)
                    .zip([containing, destination])
                {
                    if !output.accepts(commit) {
                        return Err(refused(
                            active,
                            document,
                            "Component Apply requires the exact accepted canonical checkpoint",
                        ));
                    }
                }
                Ok(())
            })
        } else {
            Err(refused(
                active,
                containing,
                "Component Apply requires the original hosted Workspace authority",
            ))
        };
        if let Err(report) = preflight {
            return Err(EditorWorkspacePrefabComponentApplyBindFailure {
                prepared,
                commits,
                report,
            });
        }
        let Some((containing_source, destination_source, saved)) =
            bind_slots(workspace, containing, destination)
        else {
            return Err(EditorWorkspacePrefabComponentApplyBindFailure {
                prepared,
                commits,
                report: refused(
                    active,
                    containing,
                    "The original Component Apply slots could not be bound",
                ),
            });
        };
        let PreparedEditorWorkspacePrefabComponentApply {
            persistence,
            scene_pair,
        } = prepared;
        match bind_prepared_prefab_component_apply(
            containing_source,
            destination_source,
            scene_pair,
        ) {
            Ok(scene_pair) => Ok(EditorWorkspacePrefabComponentApplyCommit {
                persistence,
                scene_pair,
                saved,
                commits,
            }),
            Err(failure) => {
                let (scene_pair, report) = failure.into_parts();
                Err(EditorWorkspacePrefabComponentApplyBindFailure {
                    prepared: PreparedEditorWorkspacePrefabComponentApply {
                        persistence,
                        scene_pair,
                    },
                    commits,
                    report: source_report(active, containing, report),
                })
            }
        }
    }
}

fn bind_slots(
    workspace: &mut EditorWorkspace,
    containing: EditorDocumentId,
    destination: EditorDocumentId,
) -> Option<(
    PrefabComponentApplySourceMut<'_>,
    &mut PrefabAuthoringSession,
    [BoundSavedFields<'_>; 2],
)> {
    let mut slots = workspace
        .documents
        .range_mut(containing.min(destination)..=containing.max(destination));
    let first = slots.next()?;
    let last = slots.next_back()?;
    let ((containing_id, containing_slot), (destination_id, destination_slot)) =
        if containing < destination {
            (first, last)
        } else {
            (last, first)
        };
    if *containing_id != containing || *destination_id != destination {
        return None;
    }
    let EditorDocumentSlot::Prefab(destination_slot) = destination_slot else {
        return None;
    };
    let (containing_source, containing_saved) = match containing_slot {
        EditorDocumentSlot::Scene(slot) => (
            PrefabComponentApplySourceMut::Scene(&mut slot.session),
            BoundSavedFields::new(
                &mut slot.saved_revision,
                &mut slot.saved_content_state,
                &mut slot.saved_digest,
                &mut slot.external_reload,
            ),
        ),
        EditorDocumentSlot::Prefab(slot) => {
            let (session, saved) = split_prefab_slot(slot);
            (PrefabComponentApplySourceMut::Prefab(session), saved)
        }
        EditorDocumentSlot::Extension(_) => return None,
    };
    let (destination_source, destination_saved) = split_prefab_slot(destination_slot);
    Some((
        containing_source,
        destination_source,
        [containing_saved, destination_saved],
    ))
}

fn split_prefab_slot(
    slot: &mut EditorPrefabSlot,
) -> (&mut PrefabAuthoringSession, BoundSavedFields<'_>) {
    (
        &mut slot.session,
        BoundSavedFields::new(
            &mut slot.saved_revision,
            &mut slot.saved_content_state,
            &mut slot.saved_digest,
            &mut slot.external_reload,
        ),
    )
}

fn refused(
    active: Option<EditorDocumentId>,
    document: EditorDocumentId,
    summary: &'static str,
) -> EditorWorkspaceCommandReport {
    workspace_error_report(
        WorkspaceDocumentContext::new(active, Some(document)),
        "tooling.workspace-prefab-component-apply-rejected",
        summary,
    )
}

fn source_report(
    active: Option<EditorDocumentId>,
    document: EditorDocumentId,
    report: ScenePatchReport,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        active_document: active,
        document: Some(document),
        diagnostics: report.diagnostics.clone(),
        patch_report: Some(report),
        ..EditorWorkspaceCommandReport::default()
    }
}

#[cfg(test)]
mod tests;
