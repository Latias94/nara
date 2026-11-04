use nara_text::{TextEdit, TextEditOutcome, TextEngine};
use nara_ui_kernel::{UiDispatchBatch, UiPoint};

use crate::{UiStateError, UiStateHandle, UiStateStore, UiTextField, UiTextInputToken};

/// Input admitted to one retained field incarnation and one of its focus lifetimes.
///
/// Capture this only after the Host has admitted the current field's routed focus. Retaining
/// this value grants no presentation or document authority. Eviction, explicit replacement,
/// another store or focus loss makes it unusable, even if a row occupies the same position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiStateTextInput {
    field: UiStateHandle<UiTextField>,
    token: UiTextInputToken,
}

impl UiStateStore {
    /// Reduces already admitted focus facts for directly retained first-party text fields.
    ///
    /// Returns whether a focus lifetime changed and the consuming root needs rebuilding. This
    /// scans retained slots; it is not a virtualized event index. User-defined composite state
    /// can instead retain each field separately, or reduce its own nested state explicitly.
    /// The Host still owns dispatch admission and presentation reconciliation.
    pub fn reduce_text_focus(
        &mut self,
        dispatch: &UiDispatchBatch,
        engine: &mut TextEngine,
    ) -> Result<bool, UiStateError> {
        let mut changed = false;
        for state in self.values_mut() {
            if let Some(field) = state.downcast_mut::<UiTextField>() {
                let previous = field.input_token();
                field.reduce_focus(dispatch, engine)?;
                changed |= previous != field.input_token();
            }
        }
        Ok(changed)
    }

    /// Captures the current local input lifetime, following the Host's focus admission.
    pub fn text_input(
        &self,
        field: &UiStateHandle<UiTextField>,
    ) -> Result<Option<UiStateTextInput>, UiStateError> {
        Ok(self
            .get(field)?
            .input_token()
            .map(|token| UiStateTextInput {
                field: field.clone(),
                token,
            }))
    }

    /// Applies one local edit to the exact retained field and focus that admitted it.
    ///
    /// On a changed outcome, rebuild dependent text, geometry, paint and semantics before
    /// admitting input against them. This does not apply a product value or schedule redraws.
    pub fn edit_text(
        &mut self,
        input: &UiStateTextInput,
        edit: &TextEdit,
        engine: &mut TextEngine,
    ) -> Result<TextEditOutcome, UiStateError> {
        Ok(self
            .get_mut(&input.field)?
            .edit(input.token, edit, engine)?)
    }

    /// Places or extends the caret in the field's local logical viewport after Host admission.
    pub fn point_text(
        &mut self,
        input: &UiStateTextInput,
        position: UiPoint,
        viewport_width: f32,
        extend: bool,
        engine: &mut TextEngine,
    ) -> Result<TextEditOutcome, UiStateError> {
        Ok(self.get_mut(&input.field)?.point(
            input.token,
            position,
            viewport_width,
            extend,
            engine,
        )?)
    }
}
