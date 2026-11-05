use std::sync::Arc;

use nara::{
    project_host::EditorUiAuthoringFrame,
    tooling::{EditorAction, EditorFrameModel},
};
use nara_text::{FontFace, TextEdit, TextEditOutcome};
use nara_ui_kernel::{
    UiActionId, UiDispatchBatch, UiEnvironment, UiFrame, UiKey, UiPoint, UiRect, UiRootKey, UiTheme,
};
use nara_ui_widgets::{UiAppearance, UiIntentBatch, UiTextInputToken};

use crate::panels::{PanelActionDispatch, PanelSubmissionId, PanelSubmitError};

use super::owned_hierarchy::{OwnedHierarchy, OwnedHierarchyError};
use super::owned_ui_hierarchy::{OwnedUiHierarchy, OwnedUiHierarchyError};

/// The source-declaration list and specialized UI tree have distinct domain projections.
/// Each live Hierarchy panel owns exactly one of them and shares the same native input bridge.
pub(in crate::frontend) enum HierarchyControls {
    Source(OwnedHierarchy),
    Ui(OwnedUiHierarchy),
}

pub(in crate::frontend) enum HierarchySubmission {
    Source(super::owned_hierarchy::HierarchySubmission),
    Ui(super::owned_ui_hierarchy::UiHierarchySubmission),
}

impl HierarchySubmission {
    pub(in crate::frontend) fn into_action(
        self,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        read_only: bool,
    ) -> Result<EditorAction, String> {
        match self {
            Self::Source(submission) if ui.is_none() => submission
                .into_action(model, read_only)
                .map_err(|error| error.to_string()),
            Self::Ui(submission) => submission
                .into_action(
                    model,
                    ui.ok_or("The UI source is no longer active")?,
                    read_only,
                )
                .map_err(|error| error.to_string()),
            _ => Err("The Hierarchy projection changed before submission".into()),
        }
    }
}

impl HierarchyControls {
    pub(in crate::frontend) fn set_appearance(&mut self, appearance: UiAppearance) {
        match self {
            Self::Source(owner) => owner.set_appearance(appearance),
            Self::Ui(owner) => owner.set_appearance(appearance),
        }
    }

    pub(in crate::frontend) fn new(
        root: UiRootKey,
        fonts: Vec<FontFace>,
        ui: bool,
    ) -> Result<Self, String> {
        if ui {
            OwnedUiHierarchy::new(root, fonts)
                .map(Self::Ui)
                .map_err(|error| error.to_string())
        } else {
            OwnedHierarchy::new(root, fonts)
                .map(Self::Source)
                .map_err(|error| error.to_string())
        }
    }

    pub(in crate::frontend) fn is_ui(&self) -> bool {
        matches!(self, Self::Ui(_))
    }

    pub(in crate::frontend) fn compiled_frame(&self) -> Option<&Arc<UiFrame>> {
        match self {
            Self::Source(owner) => owner.compiled_frame(),
            Self::Ui(owner) => owner.compiled_frame(),
        }
    }

    pub(in crate::frontend) fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        match self {
            Self::Source(owner) => owner.retains_frame(frame),
            Self::Ui(owner) => owner.retains_frame(frame),
        }
    }

    pub(in crate::frontend) fn owns_action(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        action: &UiActionId,
    ) -> bool {
        match self {
            Self::Source(owner) => owner.owns_action(frame, key, action),
            Self::Ui(owner) => owner.owns_action(frame, key, action),
        }
    }

    pub(in crate::frontend) fn compile(
        &mut self,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        environment: UiEnvironment,
        theme: UiTheme,
        read_only: bool,
    ) -> Result<Arc<UiFrame>, String> {
        match self {
            Self::Source(owner) if ui.is_none() => owner
                .compile(model, environment, theme, read_only)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .compile(
                    model,
                    ui.ok_or("The UI source is no longer active")?,
                    environment,
                    theme,
                    read_only,
                )
                .map_err(|error| error.to_string()),
            _ => Err("The Hierarchy projection changed".into()),
        }
    }

    pub(in crate::frontend) fn dispatch(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
    ) -> Result<UiIntentBatch, String> {
        match self {
            Self::Source(owner) => owner
                .dispatch(frame, batch)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .dispatch(frame, batch)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn activate(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        read_only: bool,
    ) -> Result<Option<HierarchySubmission>, String> {
        match self {
            Self::Source(owner) if ui.is_none() => owner
                .activate(frame, key, model, read_only)
                .map(|submission| submission.map(HierarchySubmission::Source))
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .activate(
                    frame,
                    key,
                    model,
                    ui.ok_or("The UI source is no longer active")?,
                    read_only,
                )
                .map(|submission| submission.map(HierarchySubmission::Ui))
                .map_err(|error| error.to_string()),
            _ => Err("The Hierarchy projection changed".into()),
        }
    }

    pub(in crate::frontend) fn admitted(
        &mut self,
        result: Result<PanelSubmissionId, PanelSubmitError>,
    ) {
        match self {
            Self::Source(owner) => owner.admitted(result),
            Self::Ui(owner) => owner.admitted(result),
        }
    }

    pub(in crate::frontend) fn action_finished(
        &mut self,
        id: PanelSubmissionId,
        result: &PanelActionDispatch,
    ) {
        match self {
            Self::Source(owner) => owner.action_finished(id, result),
            Self::Ui(owner) => owner.action_finished(id, result),
        }
    }

    pub(in crate::frontend) fn submission_failed(&mut self, error: String) {
        match self {
            Self::Source(owner) => owner.submission_failed(error),
            Self::Ui(owner) => owner.submission_failed(error),
        }
    }

    pub(in crate::frontend) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, String> {
        match self {
            Self::Source(owner) => owner
                .invalidated_controls(previous, candidate)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .invalidated_controls(previous, candidate)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn scroll(
        &mut self,
        frame: &Arc<UiFrame>,
        delta: f32,
    ) -> Result<bool, String> {
        match self {
            Self::Source(owner) => match owner.scroll(frame, delta) {
                Err(OwnedHierarchyError::ChangedProjection) => Ok(false),
                result => result.map_err(|error| error.to_string()),
            },
            Self::Ui(owner) => match owner.scroll(frame, delta) {
                Err(OwnedUiHierarchyError::ChangedProjection) => Ok(false),
                result => result.map_err(|error| error.to_string()),
            },
        }
    }

    pub(in crate::frontend) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<Option<UiTextInputToken>, String> {
        match self {
            Self::Source(owner) => owner
                .input_token(frame, key)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .input_token(frame, key)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn blur(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<(), String> {
        match self {
            Self::Source(owner) => owner.blur(frame, key).map_err(|error| error.to_string()),
            Self::Ui(owner) => owner.blur(frame, key).map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn cancel_input(&mut self) -> Result<(), String> {
        match self {
            Self::Source(owner) => owner.cancel_input().map_err(|error| error.to_string()),
            Self::Ui(owner) => owner.cancel_input().map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<TextEditOutcome, String> {
        match self {
            Self::Source(owner) => owner
                .edit(frame, key, token, edit)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .edit(frame, key, token, edit)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
        point: UiPoint,
        extend: bool,
    ) -> Result<TextEditOutcome, String> {
        match self {
            Self::Source(owner) => owner
                .point(frame, key, token, point, extend)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .point(frame, key, token, point, extend)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, String> {
        match self {
            Self::Source(owner) => owner
                .selected_text(frame, key, token)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .selected_text(frame, key, token)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, String> {
        match self {
            Self::Source(owner) => owner
                .is_composing(frame, key, token)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .is_composing(frame, key, token)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, String> {
        match self {
            Self::Source(owner) => owner
                .ime_area(frame, key, token)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .ime_area(frame, key, token)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        match self {
            Self::Source(owner) => owner.input_error(frame, key, token, error),
            Self::Ui(owner) => owner.input_error(frame, key, token, error),
        }
    }

    pub(in crate::frontend) fn finish_input(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        discard: bool,
    ) -> Result<(), String> {
        match self {
            Self::Source(owner) => owner
                .finish_input(frame, key, discard)
                .map_err(|error| error.to_string()),
            Self::Ui(owner) => owner
                .finish_input(frame, key, discard)
                .map_err(|error| error.to_string()),
        }
    }

    pub(in crate::frontend) fn escape(&mut self, frame: &Arc<UiFrame>) -> Result<bool, String> {
        match self {
            Self::Source(_) => Ok(false),
            Self::Ui(owner) => owner.escape(frame).map_err(|error| error.to_string()),
        }
    }
}
