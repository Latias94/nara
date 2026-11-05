use nara::tooling::{EditorFrameModel, EditorSourceToolAction, EditorSourceToolReview};

use crate::frontend::controls::source_tool::OwnedSourceToolPresentation;

use super::{SourceToolWidgets, SubmissionKind, bounded};

impl SourceToolWidgets {
    pub(in crate::frontend::imgui) fn owned_presentation(
        &self,
        frame: &EditorFrameModel,
        review: Option<&EditorSourceToolReview<'_>>,
        editable: bool,
    ) -> OwnedSourceToolPresentation {
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
        OwnedSourceToolPresentation::capture(frame, review, self.available(editable), feedback)
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
