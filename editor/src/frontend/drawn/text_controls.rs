use std::sync::Arc;

use nara_text::{TextEdit, TextEditOutcome};
use nara_ui_kernel::{UiFrame, UiKey, UiPoint, UiRect};
use nara_ui_widgets::UiTextInputToken;

use crate::EditorPanelContext;

use super::{super::controls::HierarchyControls, inspector::InspectorControls};

pub(super) enum TextControls<'a> {
    Hierarchy(&'a mut HierarchyControls),
    Inspector(&'a mut InspectorControls),
}

impl TextControls<'_> {
    pub(super) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<Option<UiTextInputToken>, String> {
        match self {
            Self::Hierarchy(owner) => owner.input_token(frame, field),
            Self::Inspector(owner) => owner.input_token(frame, field),
        }
    }

    pub(super) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, String> {
        match self {
            Self::Hierarchy(owner) => owner.is_composing(frame, field, token),
            Self::Inspector(owner) => owner.is_composing(frame, field, token),
        }
    }

    pub(super) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<TextEditOutcome, String> {
        match self {
            Self::Hierarchy(owner) => owner.edit(frame, field, token, edit),
            Self::Inspector(owner) => owner.edit(frame, field, token, edit),
        }
    }

    pub(super) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        point: UiPoint,
        extend: bool,
    ) -> Result<TextEditOutcome, String> {
        match self {
            Self::Hierarchy(owner) => owner.point(frame, field, token, point, extend),
            Self::Inspector(owner) => owner.point(frame, field, token, point, extend),
        }
    }

    pub(super) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, String> {
        match self {
            Self::Hierarchy(owner) => owner.selected_text(frame, field, token),
            Self::Inspector(owner) => owner.selected_text(frame, field, token),
        }
    }

    pub(super) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, String> {
        match self {
            Self::Hierarchy(owner) => owner.ime_area(frame, field, token),
            Self::Inspector(owner) => owner.ime_area(frame, field, token),
        }
    }

    pub(super) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        match self {
            Self::Hierarchy(owner) => owner.input_error(frame, field, token, error),
            Self::Inspector(owner) => owner.input_error(frame, field, token, error),
        }
    }

    pub(super) fn escape(&mut self, frame: &Arc<UiFrame>) -> Result<bool, String> {
        match self {
            Self::Hierarchy(owner) => owner.escape(frame),
            Self::Inspector(_) => Ok(false),
        }
    }

    pub(super) fn apply_input(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        context: &mut EditorPanelContext<'_, '_>,
    ) -> Result<(), String> {
        match self {
            Self::Hierarchy(_) => Ok(()),
            Self::Inspector(owner) => owner.finish_input(frame, field, false, context),
        }
    }

    pub(super) fn discard_input(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        context: &mut EditorPanelContext<'_, '_>,
    ) -> Result<(), String> {
        match self {
            Self::Hierarchy(owner) => owner.finish_input(frame, field, true),
            Self::Inspector(owner) => owner.finish_input(frame, field, true, context),
        }
    }
}
