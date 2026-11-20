use nara_scene::{
    AuthoringDocumentReadiness, AuthoringEntityView, AuthoringReadError, SceneEntityId,
    SourceDocumentKey,
};

use super::AdmittedAuthoringDocument;

/// A borrowed observation of the saved direct records and Schema of an unopened project source.
///
/// The Browser's `Inspected` result retains this snapshot until acknowledgement, explicit Open,
/// or Session retirement. The snapshot may be historical. Its source IDs carry no Workspace
/// mutation authority, and reads require no file I/O or runtime scene instantiation.
pub struct EditorInspectedAuthoringSource<'a> {
    pub(super) document: &'a AdmittedAuthoringDocument,
}

impl<'a> EditorInspectedAuthoringSource<'a> {
    #[must_use]
    pub fn relative_path(&self) -> &'a str {
        self.document.source().relative_path()
    }

    #[must_use]
    pub fn source_key(&self) -> &'a SourceDocumentKey {
        self.document.source().source_key()
    }

    #[must_use]
    pub fn readiness(&self) -> AuthoringDocumentReadiness {
        match self.document {
            AdmittedAuthoringDocument::Scene { document, .. } => document.readiness(),
            AdmittedAuthoringDocument::Prefab { document, .. } => document.readiness(),
        }
    }

    /// Streams direct source entities in ID order. The optional lower bound is exclusive.
    pub fn entities_after(
        &self,
        after: Option<&SceneEntityId>,
    ) -> impl ExactSizeIterator<Item = &'a SceneEntityId> + use<'a> {
        let entities = match self.document {
            AdmittedAuthoringDocument::Scene { document, .. } => &document.document().entities,
            AdmittedAuthoringDocument::Prefab { document, .. } => &document.document().entities,
        };
        let start = after.map_or(0, |after| entities.partition_point(|row| &row.id <= after));
        entities[start..].iter().map(|row| &row.id)
    }

    /// Borrows actual components and their existing classified source-field views.
    pub fn entity(
        &self,
        entity: &SceneEntityId,
    ) -> Result<AuthoringEntityView<'a>, AuthoringReadError> {
        match self.document {
            AdmittedAuthoringDocument::Scene { document, .. } => {
                document.authoring_entity_view(entity)
            }
            AdmittedAuthoringDocument::Prefab { document, .. } => {
                document.authoring_entity_view(entity)
            }
        }
    }
}
