//! Acceptance of exact encoded checkpoints from actual file results, without another write.
//!
//! The enclosing extraction owner fences writes and retains its Pending/recovery authority.
//! These mechanics establish one real file result, not a complete multi-file Ready decision.

use nara_scene::ScenePatchReport;
use nara_tooling::{EditorWorkspaceCommandReport, PreparedEditorWorkspaceExtractionPersistence};

use super::*;
use crate::project_host::file_publication::AcceptedFilePublication;

/// Transfers an actual file result into its original nominal checkpoint without another write.
/// The file remains owned even when receipt verification or the stage's acceptance refuses.
#[derive(Debug)]
pub(in crate::project_host) struct ExtractionCheckpointAcceptance {
    file: FileCapability,
    digest: ContentDigest,
    evidence: Option<EditorPersistenceReceipt>,
    state: Option<CheckpointAcceptanceState>,
}

#[derive(Debug)]
enum CheckpointAcceptanceState {
    Publication {
        encoded: EncodedEditorPersistenceCheckpoint,
        previous: FileIdentity,
        evidence: FilePublicationEvidence,
    },
    Observation(EncodedEditorPersistenceCheckpoint),
    Commit(EditorPersistenceCommit),
    Accepted,
}

/// An accepted extraction result temporarily owns its original Host's exclusive leaf guard.
/// Releasing the guard returns that same result, including its publication-only evidence.
#[must_use]
pub(in crate::project_host) struct ExtractionCheckpointLeafCommit<'a> {
    leaf: PersistenceLeafCommit<'a>,
    digest: ContentDigest,
    evidence: Option<EditorPersistenceReceipt>,
}

#[derive(Debug)]
pub(in crate::project_host) struct ExtractionCheckpointLeafBindFailure {
    acceptance: ExtractionCheckpointAcceptance,
    reason: EditorPersistenceRejection,
}

impl ExtractionCheckpointLeafCommit<'_> {
    /// Final installation only adopts the already verified leaf and moves its actual evidence.
    pub(in crate::project_host) fn commit(self) -> Option<EditorPersistenceReceipt> {
        self.leaf.commit();
        self.evidence
    }

    /// No new checkpoint, file read or publication is needed when another destination refuses.
    pub(in crate::project_host) fn into_acceptance(self) -> ExtractionCheckpointAcceptance {
        ExtractionCheckpointAcceptance {
            file: self.leaf.into_replacement(),
            digest: self.digest,
            evidence: self.evidence,
            state: Some(CheckpointAcceptanceState::Accepted),
        }
    }
}

impl std::fmt::Debug for ExtractionCheckpointLeafCommit<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExtractionCheckpointLeafCommit")
            .field("digest", &self.digest)
            .field("evidence", &self.evidence)
            .finish_non_exhaustive()
    }
}

impl ExtractionCheckpointLeafBindFailure {
    pub(in crate::project_host) fn into_parts(
        self,
    ) -> (ExtractionCheckpointAcceptance, EditorPersistenceRejection) {
        (self.acceptance, self.reason)
    }
}

impl ExtractionCheckpointAcceptance {
    pub(in crate::project_host) fn published(
        encoded: EncodedEditorPersistenceCheckpoint,
        previous: FileIdentity,
        publication: AcceptedFilePublication,
    ) -> Self {
        let evidence = editor_persistence_receipt(encoded.checkpoint(), publication.evidence());
        let (publication, source) = publication.into_parts();
        let (file, digest) = source.into_parts();
        Self {
            file,
            digest,
            evidence: Some(evidence),
            state: Some(CheckpointAcceptanceState::Publication {
                encoded,
                previous,
                evidence: publication,
            }),
        }
    }

    pub(in crate::project_host) fn observed(
        encoded: EncodedEditorPersistenceCheckpoint,
        file: FileCapability,
        digest: ContentDigest,
    ) -> Self {
        Self {
            file,
            digest,
            evidence: None,
            state: Some(CheckpointAcceptanceState::Observation(encoded)),
        }
    }

    pub(in crate::project_host) fn observation(
        &self,
    ) -> (FileIdentity, Option<FileIdentity>, ContentDigest) {
        (
            self.file.identity(),
            self.file.parent_identity(),
            self.digest,
        )
    }

    pub(in crate::project_host) fn evidence(&self) -> Option<EditorPersistenceReceipt> {
        self.evidence
    }

    pub(in crate::project_host) fn accepted(&self) -> bool {
        matches!(self.state, Some(CheckpointAcceptanceState::Accepted))
    }

    /// The enclosing operation admits native validation before binding the original Host.
    pub(in crate::project_host) fn bind_scene_leaf(
        self,
        host: &mut ScenePersistenceHost,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
    ) -> Result<ExtractionCheckpointLeafCommit<'_>, ExtractionCheckpointLeafBindFailure> {
        self.bind_leaf(|file, digest| host.bind_published_leaf(expected, file, digest))
    }

    pub(in crate::project_host) fn bind_prefab_leaf<'a>(
        self,
        host: &'a mut PrefabPersistenceHost,
        binding: &nara_tooling::EditorDocumentSourceBinding,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
    ) -> Result<ExtractionCheckpointLeafCommit<'a>, ExtractionCheckpointLeafBindFailure> {
        self.bind_leaf(|file, digest| host.bind_published_leaf(binding, expected, file, digest))
    }

    fn bind_leaf<'a>(
        self,
        bind: impl FnOnce(
            FileCapability,
            ContentDigest,
        ) -> Result<
            PersistenceLeafCommit<'a>,
            leaf_adoption::PersistenceLeafBindFailure,
        >,
    ) -> Result<ExtractionCheckpointLeafCommit<'a>, ExtractionCheckpointLeafBindFailure> {
        if !self.accepted() {
            return Err(ExtractionCheckpointLeafBindFailure {
                acceptance: self,
                reason: EditorPersistenceRejection::ResultMismatch,
            });
        }
        let Self {
            file,
            digest,
            evidence,
            state,
        } = self;
        match bind(file, digest) {
            Ok(leaf) => Ok(ExtractionCheckpointLeafCommit {
                leaf,
                digest,
                evidence,
            }),
            Err(failure) => Err(ExtractionCheckpointLeafBindFailure {
                acceptance: Self {
                    file: failure.replacement,
                    digest,
                    evidence,
                    state,
                },
                reason: failure.reason,
            }),
        }
    }

    /// The concrete owner admits the bounded hash before this observation verification runs.
    pub(in crate::project_host) fn observation_digest(&self) -> Option<ContentDigest> {
        matches!(self.state, Some(CheckpointAcceptanceState::Observation(_))).then_some(self.digest)
    }

    pub(in crate::project_host) fn advance(
        &mut self,
        stage: &mut PreparedEditorWorkspaceExtractionPersistence,
    ) -> Result<(), ExtractionPersistenceError> {
        let state = self
            .state
            .take()
            .expect("checkpoint custody remains in this owner");
        let commit = match state {
            CheckpointAcceptanceState::Publication {
                encoded,
                previous,
                evidence,
            } => {
                let (receipt, directory_sync) = evidence.into_parts();
                match EditorPersistenceCommit::from_publication(encoded, previous, receipt) {
                    Ok(commit) => commit,
                    Err(failure) => {
                        let (encoded, receipt) = failure.into_parts();
                        self.state = Some(CheckpointAcceptanceState::Publication {
                            encoded,
                            previous,
                            evidence: FilePublicationEvidence::from_parts(receipt, directory_sync),
                        });
                        return Err(ExtractionPersistenceError::Rejected(
                            EditorPersistenceRejection::SourceBindingMismatch,
                        ));
                    }
                }
            }
            CheckpointAcceptanceState::Observation(encoded) => {
                if encoded.digest() != self.digest {
                    self.state = Some(CheckpointAcceptanceState::Observation(encoded));
                    return Err(ExtractionPersistenceError::Rejected(
                        EditorPersistenceRejection::SourceBindingMismatch,
                    ));
                }
                match EditorPersistenceCommit::from_observed_file(encoded, &self.file) {
                    Ok(commit) => commit,
                    Err(failure) => {
                        let (encoded, _) = failure.into_parts();
                        self.state = Some(CheckpointAcceptanceState::Observation(encoded));
                        return Err(ExtractionPersistenceError::Rejected(
                            EditorPersistenceRejection::TargetChanged,
                        ));
                    }
                }
            }
            CheckpointAcceptanceState::Commit(commit) => commit,
            CheckpointAcceptanceState::Accepted => {
                self.state = Some(CheckpointAcceptanceState::Accepted);
                return Ok(());
            }
        };
        match stage.accept_commit(commit) {
            Ok(()) => {
                self.state = Some(CheckpointAcceptanceState::Accepted);
                Ok(())
            }
            Err(failure) => {
                let (commit, report) = failure.into_parts();
                self.state = Some(CheckpointAcceptanceState::Commit(commit));
                Err(ExtractionPersistenceError::Admission(source_report(report)))
            }
        }
    }
}

#[derive(Debug)]
pub(in crate::project_host) enum ExtractionPersistenceError {
    Admission(ScenePatchReport),
    Rejected(EditorPersistenceRejection),
}

fn source_report(report: EditorWorkspaceCommandReport) -> ScenePatchReport {
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: report.diagnostics,
    }
}
