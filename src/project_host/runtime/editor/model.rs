use std::{sync::Arc, time::Duration};

use nara_tooling::{
    EditorFrameModel, EditorProjectBrowserView, EditorProjectView, EditorReopenTarget,
};

use super::{EditorProjectSession, PublishedEditorFrame};

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
        let open_before = self.document_open_view();
        let mut scan_work_observed = match &mut self.execution {
            super::EditorExecutionLane::Authoring(project) => {
                if self.closing {
                    project.observe_source_index()
                } else {
                    project.advance_source_index(self.authoring_runtime.persistence_tasks())
                }
            }
            super::EditorExecutionLane::TrustedCompiled(_) => false,
        };
        scan_work_observed |= self.advance_document_open();
        self.advance_editor_image_import();
        self.advance_source_asset_import();
        if self.trusted_compiled().is_some() {
            self.resume_content_refresh_after_runtime_retirement();
            self.advance_project_content_refresh();
        }
        self.advance_persistence();
        self.advance_workspace_persistence();
        self.advance_workspace_retirement();
        if drive_runtime && self.trusted_compiled().is_some() {
            self.drive_play_owner(real_delta);
        }
        self.advance_workspace_retirement();
        scan_work_observed |= open_before != self.document_open_view();
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
        let source_target = project_browser
            .selected_entry()
            .map(|entry| entry.relative_path())
            .or_else(|| self.source_asset_import.target())
            .map(Arc::<str>::from);
        let source_asset_import = source_index.as_ref().and_then(|index| {
            let super::EditorExecutionLane::Authoring(project) = &self.execution else {
                return None;
            };
            let metadata = self.image_import.metadata_snapshot(index)?;
            self.source_asset_import.projection(
                project,
                index,
                &metadata,
                source_target.as_deref()?,
            )
        });
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
                .with_image_import(image_import_view, image_import_work_pending)
                .with_source_asset_import(
                    source_asset_import,
                    self.source_asset_import.has_pending_work(),
                )
                .with_document_open(self.document_open_view())
                .with_scene_preview_document(self.scene_preview_document),
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
