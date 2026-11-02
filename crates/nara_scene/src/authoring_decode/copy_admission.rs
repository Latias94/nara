//! Complete borrowed source classification for Prefab copying, without instance activation.

use std::mem::size_of;

use super::*;

#[derive(Debug)]
pub(crate) enum CopyAdmissionError<E> {
    Admission(E),
    Source(AuthoringDocumentPublicationError),
}

impl AuthoringSchemaBasis {
    /// The caller admits the complete raw Prefab before entering, including inputs, arguments
    /// and dormant declarations. Every classification allocation crosses the same owner budget.
    /// Successful source admission supplies no occurrence, input binding or executable candidate.
    pub(crate) fn admit_prefab_copy_source<E>(
        &self,
        document: &PrefabDocument,
        source: &AssetRef,
        limits: SceneFileLimits,
        mut admit: impl FnMut(&'static str, Option<usize>) -> Result<(), E>,
    ) -> Result<(), CopyAdmissionError<E>> {
        admit(
            "classification-source",
            size_of::<AuthoringDocumentState>()
                .checked_add(2 * size_of::<usize>())
                .and_then(|bytes| {
                    bytes.checked_add(crate::provenance::provenance_asset_ref_bytes(source)?)
                }),
        )
        .map_err(CopyAdmissionError::Admission)?;
        let mut refusal = None;
        let result = {
            let mut callback = |stage, bytes| {
                admit(stage, bytes).map_err(|error| {
                    refusal = Some(error);
                    // Preserve the exact owner error without allocating refusal diagnostics.
                    AuthoringDocumentPublicationError::new(DiagnosticReport::default())
                })
            };
            classify_source_document_impl(
                AuthoringSourceParts {
                    roots: &document.roots,
                    entities: &document.entities,
                    inputs: Some(&document.inputs),
                },
                AuthoringDocumentSource::new(source.clone()),
                AuthoringDocumentKind::Prefab,
                Arc::clone(&self.0),
                limits,
                Some(&mut callback),
            )
        };
        if let Some(error) = refusal {
            return Err(CopyAdmissionError::Admission(error));
        }
        let state = result.map_err(CopyAdmissionError::Source)?;
        // Overlay-target observations can remain unresolved until a real occurrence selects
        // their target/default. They do not make an otherwise interpreted record unavailable.
        if state
            .record_facts
            .iter()
            .any(|fact| !fact.readiness.is_available())
        {
            return Err(CopyAdmissionError::Source(
                AuthoringDocumentPublicationError::new(state.diagnostics),
            ));
        }
        Ok(())
    }
}
