use std::sync::Arc;

use nara_scene::{
    __private::AuthoringPreviewOmission, PrefabAuthoringSession, SceneAuthoringRevision,
};
use nara_tooling::{
    EditorDocumentId, EditorDocumentSourceBinding, EditorFrameRevision, EditorGestureCancelReason,
    EditorGestureStamp, EditorGestureView, EditorSelectionSet,
};
use nara_ui::__private::{
    UiAuthoringProjection, UiAuthoringProjectionError, UiImageChoiceGeneration, UiImageChoiceSet,
    UiStructureRejection, project_prefab_authoring,
};

use super::{
    EditorProjectSession,
    authoring_preview::{self, EditorAuthoringPreviewOmissions},
    single_error, single_warning,
};

mod mutation;

pub use mutation::EditorUiAuthoringEditError;
use mutation::EditorUiMutationAuthority;
pub(super) use mutation::{projection_admits_field_edit, projection_admits_structure_command};

/// Root-private immutable UI authoring frame shared by current and future Editor Adapters.
///
/// The Nara UI payload never owns tooling identity. This Host frame is the only place where one
/// exact Editor document/session/revision/selection fact is paired with that payload.
#[derive(Debug, Clone)]
pub struct EditorUiAuthoringFrame {
    editor_frame: EditorFrameRevision,
    document: EditorDocumentId,
    source_binding: EditorDocumentSourceBinding,
    revision: SceneAuthoringRevision,
    selection: EditorSelectionSet,
    gesture: Option<EditorGestureView>,
    last_gesture_cancellation: Option<EditorGestureCancelReason>,
    canvas_generation: Option<u64>,
    image_choices: Arc<UiImageChoiceSet>,
    mutation_authority: EditorUiMutationAuthority,
    omissions: EditorAuthoringPreviewOmissions,
    projection: Result<Arc<UiAuthoringProjection>, UiAuthoringProjectionError>,
}

impl EditorUiAuthoringFrame {
    #[must_use]
    pub const fn editor_frame(&self) -> EditorFrameRevision {
        self.editor_frame
    }

    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn source_binding(&self) -> &EditorDocumentSourceBinding {
        &self.source_binding
    }

    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn selection(&self) -> &EditorSelectionSet {
        &self.selection
    }

    #[must_use]
    pub const fn gesture(&self) -> Option<&EditorGestureView> {
        self.gesture.as_ref()
    }

    #[must_use]
    pub const fn last_gesture_cancellation(&self) -> Option<EditorGestureCancelReason> {
        self.last_gesture_cancellation
    }

    /// Returns the exact private Canvas compile generation paired with this frame.
    #[doc(hidden)]
    #[must_use]
    pub const fn canvas_generation(&self) -> Option<u64> {
        self.canvas_generation
    }

    /// Returns the exact project-content generation owning the current image catalog.
    #[doc(hidden)]
    #[must_use]
    pub fn image_choice_generation(&self) -> UiImageChoiceGeneration {
        self.image_choices.generation()
    }

    /// Returns the current bounded image catalog or its typed Host projection failure.
    #[doc(hidden)]
    pub fn image_choices(&self) -> Result<&UiImageChoiceSet, UiStructureRejection> {
        Ok(self.image_choices.as_ref())
    }

    pub fn projection(&self) -> Result<&UiAuthoringProjection, UiAuthoringProjectionError> {
        match &self.projection {
            Ok(projection) => Ok(projection.as_ref()),
            Err(error) => Err(*error),
        }
    }

    /// Returns exact source facts omitted from this fail-closed authoring projection.
    #[doc(hidden)]
    #[must_use]
    pub fn preview_omissions(&self) -> &[AuthoringPreviewOmission] {
        self.omissions.facts()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EditorUiAuthoringProjectionKey {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    gesture: Option<EditorGestureStamp>,
}

#[derive(Debug, Clone)]
pub(super) struct EditorUiAuthoringProjectionCache {
    key: EditorUiAuthoringProjectionKey,
    omissions: EditorAuthoringPreviewOmissions,
    projection: Result<Arc<UiAuthoringProjection>, UiAuthoringProjectionError>,
}

impl EditorProjectSession {
    /// Returns the UI authoring payload paired with the current Editor frame, when a Prefab is
    /// the active authoring document.
    #[doc(hidden)]
    #[must_use]
    pub fn current_ui_authoring_frame(&self) -> Option<Arc<EditorUiAuthoringFrame>> {
        self.ui_authoring_frame.as_ref().map(Arc::clone)
    }

    pub(super) fn retire_ui_authoring_after_structure_publication(
        &mut self,
        document: EditorDocumentId,
    ) {
        if self
            .ui_authoring_frame
            .as_ref()
            .is_none_or(|frame| frame.document() != document)
        {
            return;
        }
        self.ui_authoring_frame = None;
        self.ui_authoring_projection_cache = None;
        #[cfg(feature = "presentation-ui")]
        self.ui_preview.retire();
    }

    pub(super) fn reconcile_editor_gesture(&mut self) {
        let Some(active) = self.gestures.active().cloned() else {
            return;
        };
        let Some(lane) = self.trusted_compiled() else {
            let _ = self
                .gestures
                .cancel_active(EditorGestureCancelReason::SourceHandoff);
            self.last_gesture_cancellation = Some(EditorGestureCancelReason::SourceHandoff);
            return;
        };
        let target = active.baseline().target();
        let reason = if self.workspace.active_document() != Some(target.document()) {
            Some(EditorGestureCancelReason::SourceHandoff)
        } else {
            match self.workspace.document_revision(target.document()) {
                None => Some(EditorGestureCancelReason::TargetRemoved),
                Some(revision) if revision.source_id() != target.base_revision().source_id() => {
                    Some(EditorGestureCancelReason::Reopen)
                }
                Some(revision) if revision != target.base_revision() => {
                    Some(EditorGestureCancelReason::StaleRevision)
                }
                Some(_)
                    if !self.workspace.__prefab_field_target_exists(
                        lane.plan.schema_validation().registry(),
                        target,
                    ) =>
                {
                    Some(EditorGestureCancelReason::TargetRemoved)
                }
                Some(_) => None,
            }
        };
        if let Some(reason) = reason {
            let _ = self.gestures.cancel_active(reason);
            self.last_gesture_cancellation = Some(reason);
        }
    }

    pub(super) fn publish_ui_authoring_frame(&mut self, editor_frame: EditorFrameRevision) {
        let Some(document) = self.workspace.active_document() else {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
            #[cfg(feature = "presentation-ui")]
            self.ui_preview.retire();
            return;
        };
        let Some(slot) = self.workspace.prefab(document) else {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
            #[cfg(feature = "presentation-ui")]
            self.ui_preview.retire();
            return;
        };

        if !self.prefab_uses_ui_authoring_domain(document) {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
            #[cfg(feature = "presentation-ui")]
            self.ui_preview.retire();
            return;
        }

        let revision = slot.revision();
        let source_binding = slot.source_binding().clone();
        let selection = slot.selection().clone();
        #[cfg(feature = "presentation-ui")]
        let Some(canvas_source) = self
            .prefab_lanes
            .get(&document)
            .map(|lane| &lane.source_key)
        else {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
            self.ui_preview.retire();
            return;
        };
        let mut gesture = self.gestures.active().cloned();
        if gesture.as_ref().is_some_and(|gesture| {
            gesture.baseline().target().document() != document
                || gesture.baseline().target().base_revision() != revision
        }) {
            let _ = self
                .gestures
                .cancel_active(EditorGestureCancelReason::InvalidPreview);
            self.last_gesture_cancellation = Some(EditorGestureCancelReason::InvalidPreview);
            gesture = None;
        }
        let image_catalog = self.authoring_images.receipt();
        let mut key = EditorUiAuthoringProjectionKey {
            document,
            revision,
            gesture: gesture.as_ref().map(EditorGestureView::stamp),
        };
        let (projection, omissions) = match self
            .ui_authoring_projection_cache
            .as_ref()
            .filter(|cache| cache.key == key)
        {
            Some(cache) => (cache.projection.clone(), cache.omissions.clone()),
            None => {
                let (accepted, omissions) = if self.authoring_project().is_some() {
                    match authoring_preview::project_prefab(document, revision, slot.session()) {
                        Ok(projected) => projected,
                        Err(_) => {
                            self.ui_authoring_frame = None;
                            self.ui_authoring_projection_cache = None;
                            #[cfg(feature = "presentation-ui")]
                            self.ui_preview.retire();
                            let _ = self.diagnostics.extend(single_error(
                                "project.editor.authoring-ui-preview-projection-rejected",
                                "the authoring Prefab could not publish a safe UI projection",
                            ));
                            return;
                        }
                    }
                } else {
                    (
                        slot.session().document().clone(),
                        EditorAuthoringPreviewOmissions::empty(document, revision),
                    )
                };
                if !omissions.is_empty() {
                    let _ = self.diagnostics.extend(single_warning(
                        "project.editor.authoring-ui-preview-source-omitted",
                        "the UI preview omitted source values that are unavailable to generic authoring",
                    ));
                }
                let projected_document = gesture.as_ref().and_then(|gesture| {
                    let mut preview = PrefabAuthoringSession::new(accepted.clone());
                    self.trusted_compiled().and_then(|lane| {
                        preview
                            .apply_patch(
                                &gesture.preview().to_patch(),
                                lane.plan.schema_validation().registry(),
                            )
                            .applied
                            .then(|| preview.document().clone())
                    })
                });
                let projected_document = match (gesture.is_some(), projected_document.as_ref()) {
                    (true, None) => {
                        let _ = self
                            .gestures
                            .cancel_active(EditorGestureCancelReason::InvalidPreview);
                        self.last_gesture_cancellation =
                            Some(EditorGestureCancelReason::InvalidPreview);
                        gesture = None;
                        key.gesture = None;
                        &accepted
                    }
                    (_, Some(projected)) => projected,
                    (_, None) => &accepted,
                };
                let projection = project_prefab_authoring(projected_document).map(Arc::new);
                self.ui_authoring_projection_cache = Some(EditorUiAuthoringProjectionCache {
                    key,
                    omissions: omissions.clone(),
                    projection: projection.clone(),
                });
                (projection, omissions)
            }
        };
        #[cfg(feature = "presentation-ui")]
        let (projection, canvas_generation) = match projection {
            Ok(projection) => {
                let primary_surface = self
                    .authoring_project()
                    .and_then(|snapshot| snapshot.settings().ui.primary_surface)
                    .or_else(|| {
                        self.trusted_compiled()
                            .and_then(|lane| lane.plan.settings().ui.primary_surface)
                    })
                    .map(crate::product::AcceptedPrimaryUiSurfaceProfile::from_effective);
                let canvas = self.ui_preview.prepare(
                    document,
                    canvas_source,
                    revision,
                    gesture.as_ref().map(EditorGestureView::stamp),
                    projection.as_ref(),
                    image_catalog.as_ref(),
                    primary_surface,
                );
                let generation = self.ui_preview.geometry_generation();
                (
                    Ok(Arc::new(projection.as_ref().clone().with_canvas(canvas))),
                    generation,
                )
            }
            Err(error) => {
                self.ui_preview.retire();
                (Err(error), None)
            }
        };
        #[cfg(not(feature = "presentation-ui"))]
        let canvas_generation = None;
        let mutation_authority = if self.authoring_project().is_some() {
            EditorUiMutationAuthority::PreservationSafeField
        } else {
            EditorUiMutationAuthority::TrustedCompiled
        };
        self.ui_authoring_frame = Some(Arc::new(EditorUiAuthoringFrame {
            editor_frame,
            document,
            source_binding,
            revision,
            selection,
            gesture,
            last_gesture_cancellation: self.last_gesture_cancellation.take(),
            canvas_generation,
            image_choices: image_catalog.share_choices(),
            mutation_authority,
            omissions,
            projection,
        }));
    }
}
