//! Ordinary source classification for owner-verified Prefab extraction successors.
//!
//! The extraction owner separately proves original/output correspondence. This bridge retains
//! the original nominal basis, source, kind and file policy; it grants no replacement authority.

use super::{
    AuthoringDocumentKind, AuthoringDocumentState, AuthoringSourceParts, DiagnosticReport,
    PrefabDocument, SceneDocument, ScenePatchReport, diagnostic_error, reclassification,
    structure::authoring_structure_rejection,
};
use crate::format::{
    PublicationShapeAdmissionError, validate_prefab_publication_shape_with_admission,
    validate_scene_publication_shape_with_admission,
};

impl AuthoringDocumentState {
    /// Classifies the actual Scene successor after the source owner verifies its extraction.
    ///
    /// The caller must already prove complete original/output correspondence and refuse affected
    /// unavailable records. This only admits the complete output's ordinary source classification
    /// and publication shape under the original policy. Admission uses the caller's same cumulative
    /// allowance, including raw traversal, classification storage and publication scratch.
    pub(crate) fn classify_extracted_scene(
        &self,
        candidate: &SceneDocument,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        if self.kind != AuthoringDocumentKind::Scene {
            return Err(authoring_structure_rejection(
                "Scene extraction classification requires a Scene authoring source",
            ));
        }
        reclassification::admit_document(
            &candidate.placements,
            &candidate.entities,
            self.limits,
            admit,
        )?;
        validate_scene_publication_shape_with_admission(candidate, self.limits, |bytes| {
            admit("publication-buffer", Some(bytes))
        })
        .map_err(publication_error)?;
        reclassification::classify_with_admission(candidate, self, &self.schema_basis(), admit)
    }

    /// Classifies the actual Prefab successor after the source owner verifies its extraction.
    ///
    /// The caller must already prove complete original/output correspondence and refuse affected
    /// unavailable records. Borrow the Prefab's own declarations and inputs without instantiating
    /// a Scene copy. All work shares the caller's cumulative allowance and the original file policy.
    pub(crate) fn classify_extracted_prefab(
        &self,
        candidate: &PrefabDocument,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<Self, ScenePatchReport> {
        if self.kind != AuthoringDocumentKind::Prefab {
            return Err(authoring_structure_rejection(
                "Prefab extraction classification requires a Prefab authoring source",
            ));
        }
        reclassification::admit_document(
            &candidate.placements,
            &candidate.entities,
            self.limits,
            admit,
        )?;
        validate_prefab_publication_shape_with_admission(candidate, self.limits, |bytes| {
            admit("publication-buffer", Some(bytes))
        })
        .map_err(publication_error)?;
        reclassification::classify_source_with_admission(
            AuthoringSourceParts {
                placements: &candidate.placements,
                entities: &candidate.entities,
                inputs: Some(&candidate.inputs),
            },
            self,
            &self.schema_basis(),
            admit,
        )
    }
}

fn publication_error(error: PublicationShapeAdmissionError<ScenePatchReport>) -> ScenePatchReport {
    match error {
        PublicationShapeAdmissionError::Admission(report) => report,
        PublicationShapeAdmissionError::Format(_) => {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic_error(
                "scene.prefab-extraction-format-invalid",
                "Extracted source exceeds its original persistent format boundary",
            ));
            ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            }
        }
    }
}
