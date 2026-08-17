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
        self.drive_editor_frame_with_runtime(real_delta, true)
    }

    /// Publishes one coherent Editor frame while letting the native Host suppress an idle Paused
    /// Runtime drive. Lifecycle actions, Running cadence, explicit Step, resize, and recovery still
    /// pass `true`; ordinary Paused toolkit repaints pass `false` so they cannot fabricate a new
    /// unseen Runtime frame or spin the event loop.
    #[doc(hidden)]
    pub fn drive_editor_frame_with_runtime(
        &mut self,
        real_delta: Duration,
        drive_runtime: bool,
    ) -> Arc<EditorFrameModel> {
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
        if drive_runtime {
            self.drive_play_owner(real_delta);
        }
        self.advance_workspace_retirement();
        #[cfg(feature = "presentation-2d")]
        self.drive_editor_preview();
        self.publish_editor_frame()
    }

    pub(super) fn publish_editor_frame(&mut self) -> Arc<EditorFrameModel> {
        #[cfg(feature = "runtime-ui")]
        self.reconcile_editor_gesture();
        #[cfg(feature = "presentation-2d")]
        let preview_world = self.preview.world_snapshot();
        #[cfg(not(feature = "presentation-2d"))]
        let preview_world = None;
        let workspace = self
            .workspace
            .model(self.plan.schema_validation().registry(), preview_world);
        let project = self.editor_project_view();
        let diagnostics = self.diagnostics.clone();
        let reopen_document = workspace.active_document.unwrap_or(self.source_document);
        let reopen_target = match workspace.document(reopen_document) {
            Some(document) => EditorReopenTarget::OpenDocument {
                document: reopen_document,
                base_revision: document.revision,
            },
            None => EditorReopenTarget::ClosedSource {
                document: reopen_document,
            },
        };
        let frame = Arc::new(self.frame_clock.publish(
            workspace,
            project,
            diagnostics,
            reopen_target,
            self.field_feedback.clone(),
        ));
        #[cfg(feature = "runtime-ui")]
        self.publish_ui_authoring_frame(frame.revision());
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
