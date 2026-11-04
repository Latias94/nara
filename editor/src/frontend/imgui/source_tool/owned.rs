use nara::tooling::{
    EditorFrameModel, EditorSourceToolAction, EditorSourceToolRequestId, EditorSourceToolReview,
};

use super::{SourceToolWidgets, SubmissionKind, bounded, operation_label};

#[derive(Clone, Debug, PartialEq)]
pub(in crate::frontend::imgui) struct OwnedSourceToolPresentation {
    pub enabled: bool,
    pub feedback: Vec<String>,
    pub review: Option<OwnedSourceToolReview>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::frontend::imgui) struct OwnedSourceToolReview {
    pub request: EditorSourceToolRequestId,
    pub title: String,
    pub operations: Vec<String>,
}

impl SourceToolWidgets {
    pub(in crate::frontend::imgui) fn owned_presentation(
        &self,
        frame: &EditorFrameModel,
        review: Option<&EditorSourceToolReview<'_>>,
        editable: bool,
    ) -> OwnedSourceToolPresentation {
        let view = frame.source_tool();
        let mut feedback = Vec::new();
        if let Some(notice) = self.notice {
            feedback.push(bounded(&notice));
        }
        if let Some(message) = &self.feedback {
            feedback.push(bounded(message));
        }
        if self.pending.is_some() {
            feedback.push("Source-tool action queued".to_owned());
        }
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
        OwnedSourceToolPresentation {
            enabled: self.available(editable) && !view.working,
            feedback,
            review,
        }
    }

    pub(in crate::frontend::imgui) fn offer_owned(
        &mut self,
        action: EditorSourceToolAction,
    ) -> Option<EditorSourceToolAction> {
        let kind = match &action {
            EditorSourceToolAction::IntegerProperty { .. } => SubmissionKind::Integer,
            EditorSourceToolAction::Accept { .. } => SubmissionKind::Review,
            _ => return None,
        };
        self.offer(action, kind)
    }
}
