use super::*;

impl OwnedHierarchy {
    fn live_input(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<&DisplayedInput, OwnedHierarchyError> {
        let input = self
            .retained(frame)?
            .input
            .as_ref()
            .filter(|input| &input.key == field)
            .ok_or(OwnedHierarchyError::UnknownControl)?;
        if self.filter.key() != field
            || input.scope != self.scope
            || self
                .frames
                .last()
                .and_then(|compiled| compiled.input.as_ref())
                .is_none_or(|current| current.key != *field || current.scope != input.scope)
        {
            return Err(OwnedHierarchyError::ChangedProjection);
        }
        Ok(input)
    }

    fn check_token(&self, token: UiTextInputToken) -> Result<(), OwnedHierarchyError> {
        if self.filter.input_token() != Some(token) {
            return Err(UiTextFieldError::StaleInput.into());
        }
        Ok(())
    }

    pub(in crate::frontend::controls) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<Option<UiTextInputToken>, OwnedHierarchyError> {
        self.live_input(frame, field)?;
        Ok(self.filter.input_token())
    }

    pub(in crate::frontend::controls) fn blur(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<(), OwnedHierarchyError> {
        self.live_input(frame, field)?;
        self.paint_dirty = true;
        Ok(self.filter.blur(&mut self.engine)?)
    }

    pub(in crate::frontend::controls) fn cancel_input(
        &mut self,
    ) -> Result<(), OwnedHierarchyError> {
        let result = self.filter.blur(&mut self.engine);
        self.invalidate_actions();
        for control in self.controls.values_mut() {
            control.state = UiButtonState::default();
        }
        Ok(result?)
    }

    pub(in crate::frontend::controls) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<TextEditOutcome, OwnedHierarchyError> {
        self.live_input(frame, field)?;
        let result = self.filter.edit(token, edit, &mut self.engine)?;
        self.paint_dirty |= result.changed();
        if result.text_changed {
            self.source_scroll = 0.0;
            self.input_error = None;
            self.invalidate_actions();
        }
        Ok(result)
    }

    pub(in crate::frontend::controls) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        point: UiPoint,
        extend: bool,
    ) -> Result<TextEditOutcome, OwnedHierarchyError> {
        let input = self.live_input(frame, field)?;
        self.check_token(token)?;
        if input.display_text != self.filter.editor().display_text() {
            return Err(OwnedHierarchyError::ChangedProjection);
        }
        if self.filter.editor().is_composing() {
            return Err(OwnedHierarchyError::Composing);
        }
        let field_rect = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(field))
            .ok_or(OwnedHierarchyError::ChangedProjection)?
            .rect();
        let text_key = frame
            .layouts()
            .iter()
            .find(|layout| layout.key().is_some() && layout.route_parent() == Some(field))
            .and_then(|layout| layout.key())
            .ok_or(OwnedHierarchyError::ChangedProjection)?;
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
            .ok_or(OwnedHierarchyError::ChangedProjection)?;
        // Focus can restore tail scrolling before a repaint. Point into the presented text.
        let outcome = self.filter.edit(
            token,
            &TextEdit::Point {
                x: field_rect.origin().x() + point.x() - origin.x(),
                y: field_rect.origin().y() + point.y() - origin.y(),
                extend,
            },
            &mut self.engine,
        )?;
        self.paint_dirty |= outcome.changed();
        Ok(outcome)
    }

    pub(in crate::frontend::controls) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, OwnedHierarchyError> {
        self.live_input(frame, field)?;
        self.check_token(token)?;
        if self.filter.editor().is_composing() {
            return Err(OwnedHierarchyError::Composing);
        }
        let range = self.filter.editor().selection_bytes();
        Ok((!range.is_empty()).then(|| self.filter.editor().text()[range].to_owned()))
    }

    pub(in crate::frontend::controls) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, OwnedHierarchyError> {
        self.live_input(frame, field)?;
        self.check_token(token)?;
        Ok(self.filter.editor().is_composing())
    }

    pub(in crate::frontend::controls) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, OwnedHierarchyError> {
        let input = self.live_input(frame, field)?;
        self.check_token(token)?;
        Ok(self.filter.ime_area(input.width)?)
    }

    pub(in crate::frontend::controls) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        if self.input_token(frame, field).ok().flatten() == Some(token) {
            self.paint_dirty = true;
            self.input_error = Some(error);
        }
    }

    pub(in crate::frontend::controls) fn finish_input(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        discard: bool,
    ) -> Result<(), OwnedHierarchyError> {
        self.live_input(frame, field)?;
        self.paint_dirty = true;
        if discard {
            self.clear_filter()?;
        }
        self.filter.blur(&mut self.engine)?;
        Ok(())
    }

    pub(super) fn clear_filter(&mut self) -> Result<(), OwnedHierarchyError> {
        self.paint_dirty = true;
        let focused = self.filter.input_token().is_some();
        let token = self.filter.focus()?;
        self.filter
            .edit(token, &TextEdit::CancelComposition, &mut self.engine)?;
        self.filter
            .edit(token, &TextEdit::SelectAll, &mut self.engine)?;
        self.filter
            .edit(token, &TextEdit::Insert(String::new()), &mut self.engine)?;
        if !focused {
            self.filter.blur(&mut self.engine)?;
        }
        self.input_error = None;
        self.source_scroll = 0.0;
        self.invalidate_actions();
        Ok(())
    }
}
