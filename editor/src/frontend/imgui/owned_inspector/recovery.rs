use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct DisplayedRecovery {
    drafts: HashMap<FieldKey, DisplayedDraft>,
}

impl DisplayedRecovery {
    fn capture(fields: &FieldEditors) -> Self {
        Self {
            drafts: fields
                .drafts
                .iter()
                .filter(|(_, draft)| draft.dirty && !draft.in_flight())
                .map(|(key, draft)| (key.clone(), DisplayedDraft::observe(draft)))
                .collect(),
        }
    }
}

impl OwnedInspector {
    pub(in crate::frontend::imgui) fn copy_draft(
        &self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        fields: &FieldEditors,
    ) -> Result<String, OwnedInspectorError> {
        let binding = self
            .retained(frame)?
            .buttons
            .get(button)
            .filter(|button| {
                matches!(button.action, InspectorButtonAction::Copy) && button.button.enabled()
            })
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let displayed = self.live_field(frame, &binding.field)?;
        let draft = fields
            .drafts
            .get(&displayed.key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        if displayed.draft.as_ref() != Some(&DisplayedDraft::observe(draft)) {
            return Err(OwnedInspectorError::ChangedDraft);
        }
        Ok(draft.candidate.copy_text())
    }

    pub(in crate::frontend::imgui) fn copy_finished(
        &mut self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        result: Result<(), String>,
    ) -> Result<(), OwnedInspectorError> {
        let binding = self
            .retained(frame)?
            .buttons
            .get(button)
            .filter(|button| matches!(button.action, InspectorButtonAction::Copy))
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let key = self.live_field(frame, &binding.field)?.key.clone();
        let control = self
            .controls
            .get_mut(&key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        control.copy_feedback = Some(match result {
            Ok(()) => "Retained input copied".to_owned(),
            Err(error) => format!("Clipboard write failed: {error}"),
        });
        Ok(())
    }

    pub(super) fn recovery_control(
        &self,
        recovery: &DisplayedRecovery,
    ) -> Result<UiButton, OwnedInspectorError> {
        Ok(UiButton::new(
            self.recovery_button.key.clone(),
            "Discard retained field drafts",
            UiActionId::new("Discard retained field drafts")?,
        )?
        .with_enabled(!recovery.drafts.is_empty()))
    }

    pub(super) fn recovery_node(
        &self,
        fields: &FieldEditors,
        width: f32,
    ) -> Result<Option<(UiNode, DisplayedRecovery)>, OwnedInspectorError> {
        if !fields.at_capacity() {
            return Ok(None);
        }
        let recovery = DisplayedRecovery::capture(fields);
        let button = self.recovery_control(&recovery)?;
        let node = UiNode::new(UiStyle::flex().with_gap(4.0).with_flex_shrink(0.0))
            .with_key(UiLocalKey::runtime("nara.inspector.recovery", "group")?)
            .with_semantics(UiSemantics::new(UiSemanticRole::Group, "Retained drafts")?)
            .with_child(self.label("draft-capacity", &format!(
                "{} retained field drafts. Discard {} unsubmitted drafts to free capacity; queued and pending edits are preserved.",
                fields.retained_count(), recovery.drafts.len(),
            ))?)
            .with_child(button.authored_node(&self.root,
                UiStyle::flex().with_width(UiDimension::Px(width)).with_height(UiDimension::Px(30.0)).with_flex_shrink(0.0),
                UiVisual::role(button_color(self.recovery_button.state, &button)),
                UiBrush::Role(UiColorRole::PrimaryText), TEXT_SIZE,
            )?);
        Ok(Some((node, recovery)))
    }

    pub(super) fn discard_retained(
        &mut self,
        frame: &Arc<UiFrame>,
        fields: &mut FieldEditors,
    ) -> Result<(), OwnedInspectorError> {
        let recovery = self
            .retained(frame)?
            .recovery
            .as_ref()
            .ok_or(OwnedInspectorError::UnknownControl)?;
        if recovery.drafts.is_empty() || recovery != &DisplayedRecovery::capture(fields) {
            return Err(OwnedInspectorError::ChangedDraft);
        }
        let keys = recovery.drafts.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            fields.drafts.remove(&key);
            self.controls.remove(&key);
        }
        self.recovery_button.state.cancel_pointer_interaction();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
