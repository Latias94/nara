use super::*;

impl OwnedUiHierarchy {
    pub(super) fn live_input(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<&DisplayedInput, OwnedUiHierarchyError> {
        let compiled = self.retained(frame)?;
        let input = compiled
            .inputs
            .iter()
            .find(|input| &input.key == key)
            .ok_or(OwnedUiHierarchyError::UnknownControl)?;
        let form = self
            .form
            .as_ref()
            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
        if self.busy() {
            return Err(OwnedUiHierarchyError::Pending);
        }
        if self.chooser.is_some()
            || form.identity != input.form
            || !form.observation.load(Ordering::Relaxed)
            || compiled.basis != self.basis
            || self.field(input.kind)?.key() != key
            || self.frames.last().is_none_or(|latest| {
                !latest
                    .inputs
                    .iter()
                    .any(|current| current.key == *key && current.form == input.form)
            })
        {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        Ok(input)
    }

    pub(super) fn field(&self, kind: Field) -> Result<&UiTextField, OwnedUiHierarchyError> {
        self.form
            .as_ref()
            .and_then(|form| form.fields.iter().find(|field| field.kind == kind))
            .map(|field| &field.input)
            .ok_or(OwnedUiHierarchyError::UnknownControl)
    }

    pub(super) fn field_mut(
        &mut self,
        kind: Field,
    ) -> Result<(&mut UiTextField, &mut TextEngine), OwnedUiHierarchyError> {
        let field = self
            .form
            .as_mut()
            .and_then(|form| form.fields.iter_mut().find(|field| field.kind == kind))
            .ok_or(OwnedUiHierarchyError::UnknownControl)?;
        Ok((&mut field.input, &mut self.engine))
    }

    fn check_token(
        &self,
        kind: Field,
        token: UiTextInputToken,
    ) -> Result<(), OwnedUiHierarchyError> {
        if self.field(kind)?.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        Ok(())
    }

    pub(super) fn blur_fields(&mut self) -> Result<(), OwnedUiHierarchyError> {
        if let Some(form) = &mut self.form {
            for field in &mut form.fields {
                field.input.blur(&mut self.engine)?;
            }
        }
        Ok(())
    }

    pub(in crate::frontend::controls) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<Option<UiTextInputToken>, OwnedUiHierarchyError> {
        let input = self.live_input(frame, field)?;
        Ok(self.field(input.kind)?.input_token())
    }

    pub(in crate::frontend::controls) fn blur(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<(), OwnedUiHierarchyError> {
        let kind = self.live_input(frame, field)?.kind;
        let (input, engine) = self.field_mut(kind)?;
        let composing = input.editor().is_composing();
        input.blur(engine)?;
        if composing {
            self.invalidate_actions();
        }
        Ok(())
    }

    pub(in crate::frontend::controls) fn cancel_input(
        &mut self,
    ) -> Result<(), OwnedUiHierarchyError> {
        self.blur_fields()?;
        self.invalidate_actions();
        for control in self.controls.values_mut() {
            control.state = UiButtonState::default();
        }
        Ok(())
    }

    pub(in crate::frontend::controls) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<TextEditOutcome, OwnedUiHierarchyError> {
        let kind = self.live_input(frame, field)?.kind;
        let (input, engine) = self.field_mut(kind)?;
        let composing = input.editor().is_composing();
        let result = input.edit(token, edit, engine);
        match result {
            Ok(outcome) => {
                let composition_changed = composing != self.field(kind)?.editor().is_composing();
                if outcome.text_changed || composition_changed {
                    self.input_error = None;
                    self.invalidate_actions();
                }
                Ok(outcome)
            }
            Err(error) => {
                self.input_error = Some(format!(
                    "Input refused (panel field limit: {FIELD_TEXT_BYTES} UTF-8 bytes): {error}. Shorten the value and retry."
                ));
                Err(error.into())
            }
        }
    }

    pub(in crate::frontend::controls) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        point: UiPoint,
        extend: bool,
    ) -> Result<TextEditOutcome, OwnedUiHierarchyError> {
        let input = self.live_input(frame, field)?;
        let kind = input.kind;
        self.check_token(kind, token)?;
        if input.display_text != self.field(kind)?.editor().display_text() {
            return Err(OwnedUiHierarchyError::ChangedProjection);
        }
        if self.field(kind)?.editor().is_composing() {
            return Err(OwnedUiHierarchyError::Composing);
        }
        let field_rect = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(field))
            .ok_or(OwnedUiHierarchyError::ChangedProjection)?
            .rect();
        let text_key = frame
            .layouts()
            .iter()
            .find(|layout| layout.key().is_some() && layout.route_parent() == Some(field))
            .and_then(|layout| layout.key())
            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
        let origin = frame
            .paint()
            .iter()
            .find_map(|primitive| match primitive {
                nara_ui_kernel::UiPaintPrimitive::Text {
                    key: Some(key),
                    rect,
                    ..
                } if key == text_key => Some(rect.origin()),
                _ => None,
            })
            .ok_or(OwnedUiHierarchyError::ChangedProjection)?;
        let (input, engine) = self.field_mut(kind)?;
        // Refocus can restore tail scrolling before repaint; map through the presented origin.
        Ok(input.edit(
            token,
            &TextEdit::Point {
                x: field_rect.origin().x() + point.x() - origin.x(),
                y: field_rect.origin().y() + point.y() - origin.y(),
                extend,
            },
            engine,
        )?)
    }

    pub(in crate::frontend::controls) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, OwnedUiHierarchyError> {
        let kind = self.live_input(frame, field)?.kind;
        self.check_token(kind, token)?;
        let input = self.field(kind)?;
        if input.editor().is_composing() {
            return Err(OwnedUiHierarchyError::Composing);
        }
        let range = input.editor().selection_bytes();
        Ok((!range.is_empty()).then(|| input.editor().text()[range].to_owned()))
    }

    pub(in crate::frontend::controls) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, OwnedUiHierarchyError> {
        let kind = self.live_input(frame, field)?.kind;
        self.check_token(kind, token)?;
        Ok(self.field(kind)?.editor().is_composing())
    }

    pub(in crate::frontend::controls) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, OwnedUiHierarchyError> {
        let input = self.live_input(frame, field)?;
        self.check_token(input.kind, token)?;
        Ok(self.field(input.kind)?.ime_area(input.width)?)
    }

    pub(in crate::frontend::controls) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        if self.input_token(frame, field).ok().flatten() == Some(token) {
            self.input_error = Some(error);
        }
    }

    pub(in crate::frontend::controls) fn finish_input(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        discard: bool,
    ) -> Result<(), OwnedUiHierarchyError> {
        self.live_input(frame, field)?;
        if discard {
            self.cancel_form()?;
            self.error = None;
        } else {
            self.blur(frame, field)?;
        }
        Ok(())
    }
}
