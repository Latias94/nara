use super::*;
use crate::frontend::imgui::field_drafts::references::entity_choices;
use nara::tooling::SceneInspectorEntityRow;

const TARGETS_PER_PAGE: usize = 32;

impl OwnedInspector {
    pub(super) fn reference_nodes(
        &mut self,
        field: &FieldKey,
        input: &UiKey,
        rows: &[SceneInspectorEntityRow],
        enabled: bool,
        width: f32,
    ) -> Result<Vec<(UiNode, DisplayedButton)>, OwnedInspectorError> {
        let control = self
            .controls
            .get_mut(field)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let open = control.reference_open;
        let menu_key = UiKey::scoped(
            self.root.clone(),
            UiLocalKey::runtime(
                "nara.inspector.action",
                format!("{}.References", control.identity),
            )?,
        )?;
        let menu = UiButton::new(
            menu_key,
            if open {
                "Hide targets"
            } else {
                "Choose target"
            },
            UiActionId::new(InspectorOperation::References.label())?,
        )?
        .with_enabled(enabled);
        let menu_state = control.buttons[InspectorOperation::References.index()];
        let mut descriptions = vec![(
            menu,
            menu_state,
            InspectorButtonAction::Operation(InspectorOperation::References),
        )];
        if open {
            let choices = entity_choices(rows);
            let last_page = choices.len().saturating_sub(1) / TARGETS_PER_PAGE;
            control.reference_page = control.reference_page.min(last_page);
            for (operation, available) in [
                (
                    InspectorOperation::PreviousReferences,
                    control.reference_page > 0,
                ),
                (
                    InspectorOperation::NextReferences,
                    control.reference_page < last_page,
                ),
            ] {
                let key = UiKey::scoped(
                    self.root.clone(),
                    UiLocalKey::runtime(
                        "nara.inspector.action",
                        format!("{}.{}", control.identity, operation.label()),
                    )?,
                )?;
                let button =
                    UiButton::new(key, operation.label(), UiActionId::new(operation.label())?)?
                        .with_enabled(enabled && available);
                descriptions.push((
                    button,
                    control.buttons[operation.index()],
                    InspectorButtonAction::Operation(operation),
                ));
            }
            let first = control.reference_page * TARGETS_PER_PAGE;
            let choices = choices
                .into_iter()
                .skip(first)
                .take(TARGETS_PER_PAGE)
                .collect::<Vec<_>>();
            control
                .references
                .retain(|entity, _| choices.iter().any(|(id, _)| *id == entity));
            for (entity, label) in choices {
                if !control.references.contains_key(entity) {
                    let identity = self.next_identity;
                    self.next_identity = identity
                        .checked_add(1)
                        .ok_or(OwnedInspectorError::IdentityExhausted)?;
                    let key = UiKey::scoped(
                        self.root.clone(),
                        UiLocalKey::runtime("nara.inspector.reference", identity.to_string())?,
                    )?;
                    control.references.insert(
                        entity.to_owned(),
                        ButtonControl {
                            key,
                            state: UiButtonState::default(),
                        },
                    );
                }
                let reference = &control.references[entity];
                let button = UiButton::new(
                    reference.key.clone(),
                    label,
                    UiActionId::new("Choose reference")?,
                )?
                .with_enabled(enabled);
                descriptions.push((
                    button,
                    reference.state,
                    InspectorButtonAction::Reference(entity.to_owned()),
                ));
            }
        }
        descriptions
            .into_iter()
            .map(|(button, state, action)| {
                let node = button.authored_node(
                    &self.root,
                    UiStyle::flex()
                        .with_width(UiDimension::Px(width))
                        .with_height(UiDimension::Px(30.0))
                        .with_flex_shrink(0.0),
                    UiVisual::role(button_color(state, &button)),
                    UiBrush::Role(UiColorRole::PrimaryText),
                    TEXT_SIZE,
                )?;
                Ok((
                    node,
                    DisplayedButton {
                        field: input.clone(),
                        action,
                        button,
                    },
                ))
            })
            .collect()
    }

    pub(super) fn choose_reference(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        button: &UiKey,
        entity: &str,
        model: &EditorFrameModel,
        fields: &mut FieldEditors,
    ) -> Result<(), OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?.clone();
        let key = &displayed.key;
        self.check_input(key, field, fields)?;
        let control = &self.controls[key];
        if !control.reference_open
            || displayed.reference_view != Some((control.reference_open, control.reference_page))
            || control
                .references
                .get(entity)
                .is_none_or(|reference| reference.key != *button)
        {
            return Err(OwnedInspectorError::RetiredField);
        }
        if control
            .text()
            .is_some_and(|text| text.editor().is_composing())
        {
            return Err(OwnedInspectorError::Composing);
        }
        let (current, editable) =
            current_basis(model, key).ok_or(OwnedInspectorError::ChangedProjection)?;
        if !editable
            || self.read_only
            || displayed.current.as_ref() != Some(&current)
            || current.field.shape != ComponentValueShape::EntityRef
            || displayed.draft.as_ref() != Some(&DisplayedDraft::observe(&fields.drafts[key]))
            || !model
                .workspace()
                .active_scene
                .as_ref()
                .is_some_and(|scene| {
                    scene
                        .editor
                        .inspector
                        .entities
                        .iter()
                        .any(|row| row.id.as_str() == entity)
                })
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        Self::adopt_text(fields, key, None, &format!("scene-local:{entity}"))?;
        self.controls.remove(key);
        Ok(())
    }
}
