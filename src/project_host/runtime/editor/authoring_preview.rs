use std::sync::Arc;

use nara_scene::{
    __private::{
        AuthoringPreviewOmission, AuthoringPreviewProjectionError,
        project_authoring_prefab_session_preview,
    },
    PrefabAuthoringSession, PrefabDocument, SceneAuthoringRevision,
};
use nara_tooling::EditorDocumentId;

/// Exact omission facts paired with one authoring preview source revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditorAuthoringPreviewOmissions {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    facts: Arc<[AuthoringPreviewOmission]>,
}

impl EditorAuthoringPreviewOmissions {
    pub(super) fn new(
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        facts: Vec<AuthoringPreviewOmission>,
    ) -> Self {
        Self {
            document,
            revision,
            facts: Arc::from(facts),
        }
    }

    pub(super) fn empty(document: EditorDocumentId, revision: SceneAuthoringRevision) -> Self {
        Self::new(document, revision, Vec::new())
    }

    pub(super) const fn document(&self) -> EditorDocumentId {
        self.document
    }

    pub(super) const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    pub(super) fn facts(&self) -> &[AuthoringPreviewOmission] {
        &self.facts
    }

    pub(super) fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
}

pub(super) fn project_prefab(
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    session: &PrefabAuthoringSession,
) -> Result<(PrefabDocument, EditorAuthoringPreviewOmissions), AuthoringPreviewProjectionError> {
    let projection = project_authoring_prefab_session_preview(session)?;
    let (prefab, facts) = projection.into_parts();
    let omissions = EditorAuthoringPreviewOmissions::new(document, revision, facts);
    Ok((prefab, omissions))
}
