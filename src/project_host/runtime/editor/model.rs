use std::{sync::Arc, time::Duration};

use nara_tooling::{
    EditorFrameModel, EditorProjectBrowserView, EditorProjectView, EditorReopenTarget,
};

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
        let scan_work_observed = self.project_source_index_work_pending();
        if let Some(project) = self.authoring_project_mut() {
            project.advance_source_index();
        }
        self.advance_editor_image_import();
        if self.trusted_compiled().is_some() {
            self.resume_content_refresh_after_runtime_retirement();
            self.advance_project_content_refresh();
        }
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
        if drive_runtime && self.trusted_compiled().is_some() {
            self.drive_play_owner(real_delta);
        }
        self.advance_workspace_retirement();
        #[cfg(feature = "presentation-2d")]
        self.drive_editor_preview();
        self.publish_editor_frame(scan_work_observed)
    }

    pub(super) fn publish_editor_frame(
        &mut self,
        scan_work_observed: bool,
    ) -> Arc<EditorFrameModel> {
        let source_index = self.synchronize_project_browser(scan_work_observed);
        let project_browser = self.project_browser.view();
        let image_import = source_index.as_ref().and_then(|source_index| {
            let selected = project_browser.selected_entry()?;
            self.image_import.projection_for(
                source_index,
                selected.relative_path(),
                &self.authoring_runtime,
            )
        });
        let (image_import, image_import_view) = image_import
            .map(super::image_import::EditorImageImportProjection::into_parts)
            .map_or((None, None), |(private, public)| (private, Some(public)));
        #[cfg(feature = "runtime-ui")]
        self.reconcile_editor_gesture();
        #[cfg(feature = "presentation-2d")]
        let preview_world = self.authoring_runtime.preview_world_snapshot();
        #[cfg(not(feature = "presentation-2d"))]
        let preview_world = None;
        let workspace = match self.trusted_compiled() {
            Some(lane) => self
                .workspace
                .model(lane.plan.schema_validation().registry(), preview_world),
            None => self.workspace.authoring_model(),
        };
        let project = self.editor_project_view(project_browser);
        let diagnostics = self.diagnostics.clone();
        let reopen_target = workspace
            .active_document
            .and_then(|document| {
                workspace
                    .document(document)
                    .map(|facts| EditorReopenTarget::OpenDocument {
                        document,
                        base_revision: facts.revision,
                    })
            })
            .unwrap_or(EditorReopenTarget::Unavailable);
        let image_import_work_pending = self.editor_image_import_work_pending();
        let frame = Arc::new(
            self.frame_clock
                .publish(
                    workspace,
                    project,
                    diagnostics,
                    reopen_target,
                    self.field_feedback.clone(),
                )
                .with_image_import(image_import_view, image_import_work_pending),
        );
        #[cfg(feature = "runtime-ui")]
        self.publish_ui_authoring_frame(frame.revision());
        self.published_frame = Some(PublishedEditorFrame {
            model: Arc::clone(&frame),
            source_index,
            image_import,
        });
        frame
    }

    fn editor_project_view(&self, project_browser: EditorProjectBrowserView) -> EditorProjectView {
        EditorProjectView::new(
            self.project_readiness(),
            project_browser,
            self.play_view(),
            self.persistence_view(),
            self.project_content,
            self.workspace_intent_view(),
        )
        .with_inspector_results(
            self.trusted_compiled()
                .and_then(|lane| lane.runtime_edit_result.clone()),
            self.trusted_compiled()
                .and_then(|lane| lane.apply_changes_result.clone()),
        )
    }
}
