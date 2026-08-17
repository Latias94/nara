use std::sync::Arc;

use nara_scene::{PrefabAuthoringSession, SceneAuthoringRevision};
use nara_tooling::{
    EditorAtomicFieldEdit, EditorAtomicFieldEditError, EditorDocumentId,
    EditorDocumentSourceBinding, EditorFieldTarget, EditorFieldWrite, EditorFrameRevision,
    EditorGestureCancelReason, EditorGestureStamp, EditorGestureView, EditorSelectionSet,
};
use nara_ui::__private::{
    UiAuthoringCommitPolicy, UiAuthoringDirectEdit, UiAuthoringEditError, UiAuthoringFieldKey,
    UiAuthoringFieldValue, UiAuthoringProjection, UiAuthoringProjectionError,
    project_prefab_authoring,
};
use thiserror::Error;

use super::EditorProjectSession;

/// Root-private immutable UI authoring frame shared by current and future Editor Adapters.
///
/// The Nara UI payload never owns tooling identity. This Host frame is the only place where one
/// exact Editor document/session/revision/selection fact is paired with that payload.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorUiAuthoringFrame {
    editor_frame: EditorFrameRevision,
    document: EditorDocumentId,
    source_binding: EditorDocumentSourceBinding,
    revision: SceneAuthoringRevision,
    selection: EditorSelectionSet,
    gesture: Option<EditorGestureView>,
    last_gesture_cancellation: Option<EditorGestureCancelReason>,
    canvas_generation: Option<u64>,
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

    #[must_use]
    pub fn projection(&self) -> Result<&UiAuthoringProjection, UiAuthoringProjectionError> {
        match &self.projection {
            Ok(projection) => Ok(projection.as_ref()),
            Err(error) => Err(*error),
        }
    }

    /// Creates one bounded, exact-revision tooling edit from the descriptor that owns UI field
    /// semantics. Toolkit Adapters do not reproduce composite lowering rules.
    pub fn lower_field_edit(
        &self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let descriptor = projection
            .field(key)
            .ok_or(EditorUiAuthoringEditError::FieldNotInFrame)?;
        let patch = descriptor
            .lower_edit(value)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        projection
            .validate_field_patch(key, descriptor.commit_policy(), &patch)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        self.atomic_edit_from_patch(key, &patch)
    }

    /// Binds one closed two-dimensional manipulation to this exact document revision.
    ///
    /// Canonical position and size member fields remain owned by the UI authoring projection.
    pub fn lower_direct_edit(
        &self,
        edit: &UiAuthoringDirectEdit,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let (key, patch) = projection
            .lower_direct_edit(edit)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        projection
            .validate_direct_patch(&key, &patch)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        self.atomic_edit_from_patch(&key, &patch)
    }

    fn atomic_edit_from_patch(
        &self,
        key: &UiAuthoringFieldKey,
        patch: &nara_scene::ScenePatchDocument,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let descriptor = projection
            .field(key)
            .ok_or(EditorUiAuthoringEditError::FieldNotInFrame)?;
        let writes = patch
            .operations
            .iter()
            .map(|operation| {
                let nara_scene::ScenePatchOperation::SetField {
                    entity,
                    component,
                    component_version,
                    field,
                    value,
                } = operation
                else {
                    return Err(EditorUiAuthoringEditError::DescriptorMismatch);
                };
                if entity != key.entity()
                    || component != key.component()
                    || *component_version != descriptor.component_version()
                {
                    return Err(EditorUiAuthoringEditError::DescriptorMismatch);
                }
                EditorFieldWrite::new(field.clone(), value.kind(), value.clone())
                    .map_err(EditorUiAuthoringEditError::AtomicEdit)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let target = EditorFieldTarget::new(
            self.document,
            self.revision,
            key.entity().clone(),
            key.component().clone(),
            descriptor.component_version(),
            key.primary_field().clone(),
        );
        EditorAtomicFieldEdit::new(target, writes).map_err(EditorUiAuthoringEditError::AtomicEdit)
    }

    pub(super) fn admits_field_edit(
        &self,
        edit: &EditorAtomicFieldEdit,
        commit_policy: UiAuthoringCommitPolicy,
    ) -> bool {
        let target = edit.target();
        if target.document() != self.document || target.base_revision() != self.revision {
            return false;
        }
        let key = UiAuthoringFieldKey::new(
            target.entity().clone(),
            target.component().clone(),
            target.field().clone(),
        );
        self.projection().ok().is_some_and(|projection| {
            let version_matches = projection.field(&key).is_some_and(|descriptor| {
                descriptor.component_version() == target.component_version()
            });
            if !version_matches {
                return false;
            }
            let patch = edit.to_patch();
            match commit_policy {
                UiAuthoringCommitPolicy::Discrete => projection
                    .validate_field_patch(&key, commit_policy, &patch)
                    .is_ok(),
                UiAuthoringCommitPolicy::Continuous => projection
                    .validate_field_patch(&key, commit_policy, &patch)
                    .or_else(|_| projection.validate_direct_patch(&key, &patch))
                    .is_ok(),
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EditorUiAuthoringEditError {
    #[error("the UI authoring projection is unavailable: {0}")]
    ProjectionUnavailable(UiAuthoringProjectionError),
    #[error("the UI authoring field does not belong to this exact frame")]
    FieldNotInFrame,
    #[error("the UI authoring value is invalid: {0}")]
    InvalidValue(UiAuthoringEditError),
    #[error("the UI descriptor produced a mismatched patch")]
    DescriptorMismatch,
    #[error("the bounded tooling edit rejected the UI descriptor output: {0}")]
    AtomicEdit(EditorAtomicFieldEditError),
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

    pub(super) fn reconcile_editor_gesture(&mut self) {
        let Some(active) = self.gestures.active().cloned() else {
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
                        self.plan.schema_validation().registry(),
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

        let revision = slot.revision();
        let source_binding = slot.source_binding().clone();
        let selection = slot.selection().clone();
        let accepted = slot.session().document();
        let mut gesture = self.gestures.active().cloned();
        let projected_document = gesture
            .as_ref()
            .filter(|gesture| {
                gesture.baseline().target().document() == document
                    && gesture.baseline().target().base_revision() == revision
            })
            .and_then(|gesture| {
                let mut preview = PrefabAuthoringSession::new(accepted.clone());
                preview
                    .apply_patch(
                        &gesture.preview().to_patch(),
                        self.plan.schema_validation().registry(),
                    )
                    .applied
                    .then(|| preview.document().clone())
            });
        let projected_document = match (gesture.is_some(), projected_document) {
            (true, None) => {
                let _ = self
                    .gestures
                    .cancel_active(EditorGestureCancelReason::InvalidPreview);
                self.last_gesture_cancellation = Some(EditorGestureCancelReason::InvalidPreview);
                gesture = None;
                None
            }
            (_, projected) => projected,
        };
        let projected_document = projected_document.as_ref().unwrap_or(accepted);
        let key = EditorUiAuthoringProjectionKey {
            document,
            revision,
            gesture: gesture.as_ref().map(EditorGestureView::stamp),
        };
        let projection = match self
            .ui_authoring_projection_cache
            .as_ref()
            .filter(|cache| cache.key == key)
        {
            Some(cache) => cache.projection.clone(),
            None => {
                let projection = project_prefab_authoring(projected_document).map(Arc::new);
                self.ui_authoring_projection_cache = Some(EditorUiAuthoringProjectionCache {
                    key,
                    projection: projection.clone(),
                });
                projection
            }
        };
        #[cfg(feature = "presentation-ui")]
        let (projection, canvas_generation) = match projection {
            Ok(projection) => {
                let canvas = self.ui_preview.prepare(
                    document,
                    revision,
                    gesture.as_ref().map(EditorGestureView::stamp),
                    projection.as_ref(),
                    self.plan
                        .settings()
                        .ui
                        .primary_surface
                        .map(crate::product::AcceptedPrimaryUiSurfaceProfile::from_effective),
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
        let canvas_generation = {
            let _ = projected_document;
            None
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
            projection,
        }));
    }
}
