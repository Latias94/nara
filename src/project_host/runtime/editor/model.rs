use std::{sync::Arc, time::Duration};

use nara_tooling::{EditorFrameModel, EditorProjectView, EditorReopenTarget};

use super::{
    EditorPersistenceOperation, EditorProjectSession, PendingPersistence, PublishedEditorFrame,
};

impl EditorProjectSession {
    #[must_use]
    pub fn current_frame(&self) -> Arc<EditorFrameModel> {
        Arc::clone(
            &self
                .published_frame
                .as_ref()
                .expect("an opened Editor session always publishes an initial frame")
                .model,
        )
    }

    /// Drives pending Host work and publishes one immutable post-drive Editor frame.
    pub fn drive_editor_frame(&mut self, real_delta: Duration) -> Arc<EditorFrameModel> {
        if let Some(operation) = self.pending_persistence.take() {
            self.persistence_result = Some(match operation {
                PendingPersistence::Save(save) => self.complete_save(save),
                PendingPersistence::Reopen {
                    document,
                    base_revision,
                } => self.complete_reopen(document, base_revision),
            });
            self.persistence_operation = EditorPersistenceOperation::Idle;
        }
        self.advance_workspace_persistence();
        self.advance_workspace_retirement();
        self.drive_play_owner(real_delta);
        self.advance_workspace_retirement();
        #[cfg(feature = "presentation-2d")]
        self.drive_editor_preview();
        self.publish_editor_frame()
    }

    pub(super) fn publish_editor_frame(&mut self) -> Arc<EditorFrameModel> {
        #[cfg(feature = "presentation-2d")]
        let preview_world = self.preview.world_snapshot();
        #[cfg(not(feature = "presentation-2d"))]
        let preview_world = None;
        let workspace = self
            .workspace
            .model(self.plan.schema_validation().registry(), preview_world);
        let project = self.editor_project_view();
        let diagnostics = self.diagnostics.clone();
        let reopen_target = match workspace.scene(self.source_document) {
            Some(scene) => EditorReopenTarget::OpenDocument {
                document: self.source_document,
                base_revision: scene.revision,
            },
            None => EditorReopenTarget::ClosedSource {
                document: self.source_document,
            },
        };
        let frame = Arc::new(self.frame_clock.publish(
            workspace,
            project,
            diagnostics,
            reopen_target,
            self.field_feedback.clone(),
        ));
        self.published_frame = Some(PublishedEditorFrame {
            model: Arc::clone(&frame),
            dispatched: false,
        });
        frame
    }

    fn editor_project_view(&self) -> EditorProjectView {
        EditorProjectView::new(
            self.play_view(),
            self.persistence_view(),
            self.workspace_intent_view(),
        )
        .with_inspector_results(
            self.runtime_edit_result.clone(),
            self.apply_changes_result.clone(),
        )
    }
}
