use egui::{TextureId, Vec2};
use nara_scene::{SceneAuthoringRevision, SceneEntityId};
use nara_tooling::{
    EditorAtomicFieldEdit, EditorDocumentId, EditorDocumentMutation, EditorGestureView,
    EditorSelectionSet,
};
use nara_ui::__private::{
    UiAuthoringDirectEdit, UiAuthoringFieldKey, UiAuthoringFieldValue, UiAuthoringProjection,
    UiAuthoringProjectionError, UiImageChoiceGeneration, UiImageChoiceSet, UiStructureIntent,
    UiStructureRejection,
};

/// Redraw-local image catalog facts for one exact UI authoring frame.
#[doc(hidden)]
pub struct EguiUiAuthoringImages<'a> {
    image_choice_generation: UiImageChoiceGeneration,
    image_choices: Result<&'a UiImageChoiceSet, UiStructureRejection>,
}

impl<'a> EguiUiAuthoringImages<'a> {
    #[must_use]
    pub const fn new(
        image_choice_generation: UiImageChoiceGeneration,
        image_choices: Result<&'a UiImageChoiceSet, UiStructureRejection>,
    ) -> Self {
        Self {
            image_choice_generation,
            image_choices,
        }
    }
}

/// Redraw-local mutation port supplied by the concrete Editor Host.
#[doc(hidden)]
pub struct EguiUiAuthoringMutations<'a> {
    field_editing_enabled: &'a dyn for<'key> Fn(&'key UiAuthoringFieldKey) -> bool,
    direct_editing_enabled: bool,
    structure_editing_enabled: bool,
    lower_continuous_field_edit:
        &'a mut dyn for<'key> FnMut(
            &'key UiAuthoringFieldKey,
            UiAuthoringFieldValue,
        ) -> Result<EditorAtomicFieldEdit, String>,
    lower_discrete_field_edit:
        &'a mut dyn for<'key> FnMut(
            &'key UiAuthoringFieldKey,
            UiAuthoringFieldValue,
        ) -> Result<EditorDocumentMutation, String>,
    lower_direct_edit: &'a mut dyn for<'edit> FnMut(
        &'edit UiAuthoringDirectEdit,
    ) -> Result<EditorAtomicFieldEdit, String>,
    lower_structure_intent:
        &'a mut dyn for<'intent> FnMut(
            &'intent UiStructureIntent,
        ) -> Result<EditorDocumentMutation, String>,
}

impl<'a> EguiUiAuthoringMutations<'a> {
    #[must_use]
    pub fn new(
        field_editing_enabled: &'a dyn for<'key> Fn(&'key UiAuthoringFieldKey) -> bool,
        direct_editing_enabled: bool,
        structure_editing_enabled: bool,
        lower_continuous_field_edit: &'a mut dyn for<'key> FnMut(
            &'key UiAuthoringFieldKey,
            UiAuthoringFieldValue,
        ) -> Result<
            EditorAtomicFieldEdit,
            String,
        >,
        lower_discrete_field_edit: &'a mut dyn for<'key> FnMut(
            &'key UiAuthoringFieldKey,
            UiAuthoringFieldValue,
        ) -> Result<
            EditorDocumentMutation,
            String,
        >,
        lower_direct_edit: &'a mut dyn for<'edit> FnMut(
            &'edit UiAuthoringDirectEdit,
        )
            -> Result<EditorAtomicFieldEdit, String>,
        lower_structure_intent: &'a mut dyn for<'intent> FnMut(
            &'intent UiStructureIntent,
        ) -> Result<
            EditorDocumentMutation,
            String,
        >,
    ) -> Self {
        Self {
            field_editing_enabled,
            direct_editing_enabled,
            structure_editing_enabled,
            lower_continuous_field_edit,
            lower_discrete_field_edit,
            lower_direct_edit,
            lower_structure_intent,
        }
    }
}

/// Borrowed first-party Adapter input for one exact UI authoring frame.
///
/// Document identity and lowering remain Host-owned. This value is valid only for one egui redraw.
pub struct EguiUiAuthoringInput<'a> {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    selection: &'a EditorSelectionSet,
    gesture: Option<&'a EditorGestureView>,
    projection: Result<&'a UiAuthoringProjection, UiAuthoringProjectionError>,
    images: EguiUiAuthoringImages<'a>,
    mutations: EguiUiAuthoringMutations<'a>,
}

impl<'a> EguiUiAuthoringInput<'a> {
    /// Creates one redraw-local Adapter input from the concrete Editor Host.
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        selection: &'a EditorSelectionSet,
        gesture: Option<&'a EditorGestureView>,
        projection: Result<&'a UiAuthoringProjection, UiAuthoringProjectionError>,
        images: EguiUiAuthoringImages<'a>,
        mutations: EguiUiAuthoringMutations<'a>,
    ) -> Self {
        Self {
            document,
            revision,
            selection,
            gesture,
            projection,
            images,
            mutations,
        }
    }

    pub(crate) const fn document(&self) -> EditorDocumentId {
        self.document
    }

    pub(crate) const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    fn projection(&self) -> Result<&UiAuthoringProjection, UiAuthoringProjectionError> {
        self.projection
    }

    fn image_choice_generation(&self) -> UiImageChoiceGeneration {
        self.images.image_choice_generation
    }

    fn image_choices(&self) -> Result<&UiImageChoiceSet, UiStructureRejection> {
        self.images.image_choices
    }

    fn selected_entity(&self) -> Option<&SceneEntityId> {
        self.selection.top_entity()
    }

    fn gesture(&self) -> Option<&EditorGestureView> {
        self.gesture
    }

    fn field_editing_enabled(&self, key: &UiAuthoringFieldKey) -> bool {
        (self.mutations.field_editing_enabled)(key)
    }

    const fn direct_editing_enabled(&self) -> bool {
        self.mutations.direct_editing_enabled
    }

    const fn structure_editing_enabled(&self) -> bool {
        self.mutations.structure_editing_enabled
    }

    fn lower_continuous_field_edit(
        &mut self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorAtomicFieldEdit, String> {
        (self.mutations.lower_continuous_field_edit)(key, value)
    }

    fn lower_discrete_field_edit(
        &mut self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorDocumentMutation, String> {
        (self.mutations.lower_discrete_field_edit)(key, value)
    }

    fn lower_direct_edit(
        &mut self,
        edit: &UiAuthoringDirectEdit,
    ) -> Result<EditorAtomicFieldEdit, String> {
        (self.mutations.lower_direct_edit)(edit)
    }

    fn lower_structure_intent(
        &mut self,
        intent: &UiStructureIntent,
    ) -> Result<EditorDocumentMutation, String> {
        (self.mutations.lower_structure_intent)(intent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiUiCanvasContent {
    Presented {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Previewing {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Resizing,
    Unavailable,
}

/// Toolkit-local texture facts for one UI Canvas redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EguiUiCanvasInput {
    content: EguiUiCanvasContent,
    notice: Option<&'static str>,
}

impl EguiUiCanvasInput {
    #[must_use]
    pub const fn presented(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiUiCanvasContent::Presented { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn previewing(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiUiCanvasContent::Previewing { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn resizing() -> Self {
        Self {
            content: EguiUiCanvasContent::Resizing,
            notice: None,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            content: EguiUiCanvasContent::Unavailable,
            notice: None,
        }
    }

    #[must_use]
    pub const fn with_notice(mut self, notice: &'static str) -> Self {
        self.notice = Some(notice);
        self
    }

    const fn is_presented(self) -> bool {
        matches!(self.content, EguiUiCanvasContent::Presented { .. })
    }

    const fn keeps_gesture_alive(self) -> bool {
        self.notice.is_none()
            && matches!(
                self.content,
                EguiUiCanvasContent::Presented { .. } | EguiUiCanvasContent::Previewing { .. }
            )
    }
}

impl Default for EguiUiCanvasInput {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// Transient interaction emitted by one fixed-fit UI Canvas redraw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EguiUiCanvasOutput {
    desired_size_points: Vec2,
    pick_uv: Option<[f32; 2]>,
}

impl EguiUiCanvasOutput {
    #[must_use]
    pub const fn desired_size_points(self) -> Vec2 {
        self.desired_size_points
    }

    #[must_use]
    pub const fn pick_uv(self) -> Option<[f32; 2]> {
        self.pick_uv
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EguiUiAuthoringScope {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    image_choice_generation: UiImageChoiceGeneration,
}

mod canvas;
mod inspector;

pub(crate) use canvas::EguiUiCanvasPanel;
pub(crate) use inspector::EguiUiAuthoringPanel;
