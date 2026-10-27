//! Selection-independent access to declarations owned by an exact source session.

use nara_scene::{PrefabDeclarationLimits, PrefabInstanceObservation, PrefabInstancePath};

use super::{EditorDocumentSlot, EditorWorkspace};
use crate::{EditorDocumentQueryBasis, EditorDocumentQueryRejection};

impl EditorWorkspace {
    /// Observes an optional source declaration without changing navigation or selection.
    ///
    /// The exact query basis is checked before any selector or payload is read. The source
    /// session owns bounded copying; this does not resolve inherited entities or Prefab sources.
    pub fn observe_prefab_instance(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, EditorDocumentQueryRejection> {
        let slot = self.resolve_document_query_slot(basis, None)?;
        let observation = match slot {
            EditorDocumentSlot::Scene(slot) => slot.session.observe_prefab_instance(path, limits),
            EditorDocumentSlot::Prefab(slot) => slot.session.observe_prefab_instance(path, limits),
            EditorDocumentSlot::Extension(_) => {
                return Err(EditorDocumentQueryRejection::UnsupportedDocumentKind {
                    kind: slot.kind(),
                });
            }
        };
        observation.map_err(EditorDocumentQueryRejection::PrefabDeclaration)
    }
}
