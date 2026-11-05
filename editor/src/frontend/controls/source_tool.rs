//! Read-only source-tool values used by the owned Inspector and reference painter.

use nara::{
    scene::ScenePatchOperation,
    tooling::{EditorFrameModel, EditorSourceToolRequestId, EditorSourceToolReview},
};

use super::authoring::value_label;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::frontend) struct OwnedSourceToolPresentation {
    pub(in crate::frontend) enabled: bool,
    pub(in crate::frontend) feedback: Vec<String>,
    pub(in crate::frontend) review: Option<OwnedSourceToolReview>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::frontend) struct OwnedSourceToolReview {
    pub(in crate::frontend) request: EditorSourceToolRequestId,
    pub(in crate::frontend) title: String,
    pub(in crate::frontend) operations: Vec<String>,
}

impl OwnedSourceToolPresentation {
    /// The frontend supplies its local submission availability and feedback. The Session retains
    /// the proposal and review; only the exact current request is exposed for presentation.
    pub(in crate::frontend) fn capture(
        frame: &EditorFrameModel,
        review: Option<&EditorSourceToolReview<'_>>,
        enabled: bool,
        feedback: Vec<String>,
    ) -> Self {
        let view = frame.source_tool();
        let review = view
            .review
            .as_ref()
            .and_then(|summary| {
                review.filter(|review| {
                    review.request() == &summary.request
                        && view.request.as_ref() == Some(review.request())
                })
            })
            .map(|review| OwnedSourceToolReview {
                request: review.request().clone(),
                title: review.title().to_owned(),
                operations: review
                    .patch()
                    .operations
                    .iter()
                    .map(operation_label)
                    .collect(),
            });
        Self {
            enabled: enabled && !view.working,
            feedback,
            review,
        }
    }
}

pub(in crate::frontend) fn operation_label(operation: &ScenePatchOperation) -> String {
    match operation {
        ScenePatchOperation::SetField {
            entity,
            component,
            field,
            value,
            ..
        } => format!(
            "{} / {} / {}: {}",
            entity.as_str(),
            component.as_str(),
            field.as_str(),
            value_label(Some(value))
        ),
        ScenePatchOperation::AddEntity { entity } => {
            format!("Create entity {}", entity.id.as_str())
        }
        ScenePatchOperation::SetPlacements { .. } => "Update source hierarchy".to_owned(),
        ScenePatchOperation::AddComponent {
            entity, component, ..
        } => format!("Add {} to {}", component.as_str(), entity.as_str()),
        other => format!("{other:?}"),
    }
}
