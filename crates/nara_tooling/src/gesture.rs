//! UI-neutral bounded field edits and continuous authoring transactions.

use std::{collections::BTreeSet, error::Error, fmt};

use nara_reflect::{ComponentFieldId, ComponentValue, ComponentValueKind};
use nara_scene::{ScenePatchDocument, ScenePatchOperation};

use crate::{EditorDocumentId, EditorFieldTarget};

pub const MAX_EDITOR_ATOMIC_FIELD_WRITES: usize = 8;
const MAX_EDITOR_ATOMIC_VALUE_NODES: usize = 256;
const MAX_EDITOR_ATOMIC_LOGICAL_BYTES: usize = 64 * 1024;

/// One exact canonical field write within an atomic edit.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorFieldWrite {
    field: ComponentFieldId,
    value_kind: ComponentValueKind,
    value: ComponentValue,
}

impl EditorFieldWrite {
    pub fn new(
        field: ComponentFieldId,
        value_kind: ComponentValueKind,
        value: ComponentValue,
    ) -> Result<Self, EditorAtomicFieldEditError> {
        if value.kind() != value_kind {
            return Err(EditorAtomicFieldEditError::ValueKindMismatch);
        }
        Ok(Self {
            field,
            value_kind,
            value,
        })
    }

    #[must_use]
    pub const fn field(&self) -> &ComponentFieldId {
        &self.field
    }

    #[must_use]
    pub const fn value_kind(&self) -> ComponentValueKind {
        self.value_kind
    }

    #[must_use]
    pub const fn value(&self) -> &ComponentValue {
        &self.value
    }
}

/// One bounded failure-atomic field edit over one exact component target.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorAtomicFieldEdit {
    target: EditorFieldTarget,
    writes: Vec<EditorFieldWrite>,
}

impl EditorAtomicFieldEdit {
    pub fn new(
        target: EditorFieldTarget,
        writes: impl IntoIterator<Item = EditorFieldWrite>,
    ) -> Result<Self, EditorAtomicFieldEditError> {
        let writes = writes.into_iter().collect::<Vec<_>>();
        if writes.is_empty() {
            return Err(EditorAtomicFieldEditError::Empty);
        }
        if writes.len() > MAX_EDITOR_ATOMIC_FIELD_WRITES {
            return Err(EditorAtomicFieldEditError::WriteLimitExceeded);
        }
        let mut fields = BTreeSet::new();
        let mut value_nodes = 0usize;
        let mut logical_bytes = 0usize;
        for write in &writes {
            if !fields.insert(write.field.clone()) {
                return Err(EditorAtomicFieldEditError::DuplicateField);
            }
            let cost = write.value.cost();
            value_nodes = value_nodes.saturating_add(cost.nodes());
            logical_bytes = logical_bytes.saturating_add(cost.logical_bytes());
        }
        if !fields.contains(target.field()) {
            return Err(EditorAtomicFieldEditError::PrimaryFieldMissing);
        }
        if value_nodes > MAX_EDITOR_ATOMIC_VALUE_NODES
            || logical_bytes > MAX_EDITOR_ATOMIC_LOGICAL_BYTES
        {
            return Err(EditorAtomicFieldEditError::ValueBudgetExceeded);
        }
        Ok(Self { target, writes })
    }

    #[must_use]
    pub const fn target(&self) -> &EditorFieldTarget {
        &self.target
    }

    #[must_use]
    pub fn writes(&self) -> &[EditorFieldWrite] {
        &self.writes
    }

    #[must_use]
    pub fn same_shape(&self, other: &Self) -> bool {
        self.target == other.target
            && self.writes.len() == other.writes.len()
            && self.writes.iter().zip(&other.writes).all(|(left, right)| {
                left.field == right.field && left.value_kind == right.value_kind
            })
    }

    #[must_use]
    pub fn to_patch(&self) -> ScenePatchDocument {
        ScenePatchDocument::new(
            self.writes
                .iter()
                .map(|write| ScenePatchOperation::SetField {
                    entity: self.target.entity().clone(),
                    component: self.target.component().clone(),
                    component_version: self.target.component_version(),
                    field: write.field.clone(),
                    value: write.value.clone(),
                }),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAtomicFieldEditError {
    Empty,
    WriteLimitExceeded,
    DuplicateField,
    PrimaryFieldMissing,
    ValueKindMismatch,
    ValueBudgetExceeded,
}

impl fmt::Display for EditorAtomicFieldEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "an atomic field edit must contain at least one write",
            Self::WriteLimitExceeded => "an atomic field edit exceeds the bounded write ceiling",
            Self::DuplicateField => "an atomic field edit contains the same field more than once",
            Self::PrimaryFieldMissing => "an atomic field edit does not include its primary field",
            Self::ValueKindMismatch => {
                "an atomic field edit value does not match its declared value kind"
            }
            Self::ValueBudgetExceeded => {
                "an atomic field edit exceeds its value-node or logical-byte budget"
            }
        })
    }
}

impl Error for EditorAtomicFieldEditError {}

/// Host-assigned identity of one continuous edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorGestureId(u64);

impl EditorGestureId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Exact transient preview generation of one gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorGestureStamp {
    id: EditorGestureId,
    preview_generation: u64,
}

impl EditorGestureStamp {
    #[must_use]
    pub const fn id(self) -> EditorGestureId {
        self.id
    }

    #[must_use]
    pub const fn preview_generation(self) -> u64 {
        self.preview_generation
    }
}

/// Explicit reason a gesture was cancelled without document publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorGestureCancelReason {
    Escape,
    FocusLost,
    SourceHandoff,
    Reopen,
    TargetRemoved,
    StaleRevision,
    TargetRetired,
    InvalidPreview,
}

/// Ordered continuous-edit action emitted by any Editor Adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorGestureAction {
    Begin {
        baseline: EditorAtomicFieldEdit,
    },
    Preview {
        gesture: EditorGestureId,
        edit: EditorAtomicFieldEdit,
    },
    Commit {
        gesture: EditorGestureId,
        edit: EditorAtomicFieldEdit,
    },
    Cancel {
        gesture: EditorGestureId,
        reason: EditorGestureCancelReason,
    },
}

impl EditorGestureAction {
    #[must_use]
    pub const fn field_target(&self) -> Option<&EditorFieldTarget> {
        match self {
            Self::Begin { baseline } => Some(baseline.target()),
            Self::Preview { edit, .. } | Self::Commit { edit, .. } => Some(edit.target()),
            Self::Cancel { .. } => None,
        }
    }

    #[must_use]
    pub const fn revision_advancing_document(&self) -> Option<EditorDocumentId> {
        match self {
            Self::Commit { edit, .. } => Some(edit.target().document()),
            Self::Begin { .. } | Self::Preview { .. } | Self::Cancel { .. } => None,
        }
    }
}

/// Immutable active-gesture facts published beside an Editor authoring frame.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorGestureView {
    stamp: EditorGestureStamp,
    baseline: EditorAtomicFieldEdit,
    preview: EditorAtomicFieldEdit,
}

impl EditorGestureView {
    #[must_use]
    pub const fn stamp(&self) -> EditorGestureStamp {
        self.stamp
    }

    #[must_use]
    pub const fn baseline(&self) -> &EditorAtomicFieldEdit {
        &self.baseline
    }

    #[must_use]
    pub const fn preview(&self) -> &EditorAtomicFieldEdit {
        &self.preview
    }
}

/// Typed gesture failure; no rejection advances transient or document state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorGestureRejection {
    AlreadyActive,
    NoActiveGesture,
    GestureMismatch,
    FieldShapeMismatch,
    BaselineMismatch,
    GenerationExhausted,
}

/// Outcome returned by the concrete Host for one gesture action.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorGestureOutcome {
    Began(EditorGestureView),
    Previewed(EditorGestureView),
    Committed,
    Cancelled(EditorGestureCancelReason),
    Rejected(EditorGestureRejection),
}

/// Single-active-gesture owner. The Editor Host stores this; toolkit Adapters never do.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct EditorGestureController {
    next_id: u64,
    active: Option<EditorGestureView>,
}

impl EditorGestureController {
    #[must_use]
    pub fn active(&self) -> Option<&EditorGestureView> {
        self.active.as_ref()
    }

    pub fn validate_edit(
        &self,
        gesture: EditorGestureId,
        edit: &EditorAtomicFieldEdit,
    ) -> Result<(), EditorGestureRejection> {
        let Some(active) = self.active.as_ref() else {
            return Err(EditorGestureRejection::NoActiveGesture);
        };
        if active.stamp.id != gesture {
            return Err(EditorGestureRejection::GestureMismatch);
        }
        if !active.baseline.same_shape(edit) {
            return Err(EditorGestureRejection::FieldShapeMismatch);
        }
        Ok(())
    }

    pub fn begin(&mut self, baseline: EditorAtomicFieldEdit) -> EditorGestureOutcome {
        if self.active.is_some() {
            return EditorGestureOutcome::Rejected(EditorGestureRejection::AlreadyActive);
        }
        let Some(next_id) = self.next_id.checked_add(1) else {
            return EditorGestureOutcome::Rejected(EditorGestureRejection::GenerationExhausted);
        };
        self.next_id = next_id;
        let view = EditorGestureView {
            stamp: EditorGestureStamp {
                id: EditorGestureId(next_id),
                preview_generation: 0,
            },
            preview: baseline.clone(),
            baseline,
        };
        self.active = Some(view.clone());
        EditorGestureOutcome::Began(view)
    }

    pub fn preview(
        &mut self,
        gesture: EditorGestureId,
        edit: EditorAtomicFieldEdit,
    ) -> EditorGestureOutcome {
        if let Err(rejection) = self.validate_edit(gesture, &edit) {
            return EditorGestureOutcome::Rejected(rejection);
        }
        let active = self
            .active
            .as_mut()
            .expect("validated gestures retain their active transaction");
        let Some(generation) = active.stamp.preview_generation.checked_add(1) else {
            return EditorGestureOutcome::Rejected(EditorGestureRejection::GenerationExhausted);
        };
        active.stamp.preview_generation = generation;
        active.preview = edit;
        EditorGestureOutcome::Previewed(active.clone())
    }

    pub fn commit(
        &mut self,
        gesture: EditorGestureId,
        edit: EditorAtomicFieldEdit,
    ) -> Result<EditorAtomicFieldEdit, EditorGestureRejection> {
        self.validate_edit(gesture, &edit)?;
        self.active = None;
        Ok(edit)
    }

    pub fn cancel(
        &mut self,
        gesture: EditorGestureId,
        reason: EditorGestureCancelReason,
    ) -> EditorGestureOutcome {
        let Some(active) = self.active.as_ref() else {
            return EditorGestureOutcome::Rejected(EditorGestureRejection::NoActiveGesture);
        };
        if active.stamp.id != gesture {
            return EditorGestureOutcome::Rejected(EditorGestureRejection::GestureMismatch);
        }
        self.active = None;
        EditorGestureOutcome::Cancelled(reason)
    }

    pub fn cancel_active(
        &mut self,
        reason: EditorGestureCancelReason,
    ) -> Option<EditorGestureOutcome> {
        self.active
            .take()
            .map(|_| EditorGestureOutcome::Cancelled(reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_reflect::{ComponentSchemaVersion, ComponentTypeId};
    use nara_scene::{SceneAuthoringRevision, SceneEntityId};

    fn edit(value: f64, revision: SceneAuthoringRevision) -> EditorAtomicFieldEdit {
        let target = EditorFieldTarget::new(
            EditorDocumentId::from_raw(1),
            revision,
            SceneEntityId::new("button").unwrap(),
            ComponentTypeId::new("nara.ui.UiNode"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("style.left.value"),
        );
        EditorAtomicFieldEdit::new(
            target,
            [EditorFieldWrite::new(
                ComponentFieldId::new("style.left.value"),
                ComponentValueKind::F64,
                ComponentValue::f64(value).unwrap(),
            )
            .unwrap()],
        )
        .unwrap()
    }

    use nara_scene::PrefabDocument;

    #[test]
    fn previews_are_transient_and_commit_returns_one_atomic_edit() {
        let mut controller = EditorGestureController::default();
        let revision =
            nara_scene::PrefabAuthoringSession::new(PrefabDocument::default()).revision();
        let began = controller.begin(edit(10.0, revision));
        let EditorGestureOutcome::Began(began) = began else {
            panic!("gesture must begin");
        };
        let gesture = began.stamp().id();
        let first = controller.preview(gesture, edit(20.0, revision));
        let second = controller.preview(gesture, edit(30.0, revision));
        assert!(matches!(first, EditorGestureOutcome::Previewed(_)));
        assert!(matches!(second, EditorGestureOutcome::Previewed(_)));
        assert_eq!(controller.active().unwrap().stamp().preview_generation(), 2);
        let committed = controller.commit(gesture, edit(40.0, revision)).unwrap();
        assert_eq!(committed.to_patch().operations.len(), 1);
        assert!(controller.active().is_none());
    }

    #[test]
    fn cancel_retires_preview_without_a_patch() {
        let mut controller = EditorGestureController::default();
        let revision =
            nara_scene::PrefabAuthoringSession::new(PrefabDocument::default()).revision();
        let EditorGestureOutcome::Began(view) = controller.begin(edit(10.0, revision)) else {
            panic!("gesture must begin");
        };
        assert_eq!(
            controller.cancel(view.stamp().id(), EditorGestureCancelReason::Escape),
            EditorGestureOutcome::Cancelled(EditorGestureCancelReason::Escape)
        );
        assert!(controller.active().is_none());
    }

    #[test]
    fn rejected_gesture_edits_preserve_the_active_transaction() {
        let mut controller = EditorGestureController::default();
        let revision =
            nara_scene::PrefabAuthoringSession::new(PrefabDocument::default()).revision();
        assert_eq!(
            controller.preview(EditorGestureId(1), edit(20.0, revision)),
            EditorGestureOutcome::Rejected(EditorGestureRejection::NoActiveGesture)
        );

        let EditorGestureOutcome::Began(view) = controller.begin(edit(10.0, revision)) else {
            panic!("gesture must begin");
        };
        let retained = view.clone();
        assert_eq!(
            controller.preview(
                EditorGestureId(view.stamp().id().get() + 1),
                edit(20.0, revision)
            ),
            EditorGestureOutcome::Rejected(EditorGestureRejection::GestureMismatch)
        );
        let other_revision =
            nara_scene::PrefabAuthoringSession::new(PrefabDocument::default()).revision();
        assert_eq!(
            controller.preview(view.stamp().id(), edit(20.0, other_revision)),
            EditorGestureOutcome::Rejected(EditorGestureRejection::FieldShapeMismatch)
        );
        assert_eq!(controller.active(), Some(&retained));
    }
}
