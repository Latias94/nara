use super::*;
use structure::StructuredRow;

#[derive(Default)]
pub(super) struct StructuredControls {
    pub(super) inputs: HashMap<DraftInput, Box<UiTextField>>,
    pub(super) buttons: HashMap<DraftAction, ButtonControl>,
    pub(super) references: HashMap<DraftInput, ReferencePicker>,
}

impl StructuredControls {
    fn synchronize(&mut self, rows: &[StructuredRow]) {
        let inputs = rows
            .iter()
            .filter_map(|row| match row {
                StructuredRow::Text { input, text, .. } => Some((input, text.as_str())),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let actions = rows
            .iter()
            .filter_map(|row| match row {
                StructuredRow::Button { action, .. } => Some(action),
                _ => None,
            })
            .collect::<HashSet<_>>();
        self.inputs
            .retain(|target, control| inputs.get(target).copied() == Some(control.editor().text()));
        self.buttons.retain(|target, _| actions.contains(target));
        self.references
            .retain(|target, _| inputs.contains_key(target));
    }
}

impl OwnedInspector {
    pub(super) fn structured_nodes(
        &mut self,
        key: &FieldKey,
        draft: &FieldDraft,
        width: f32,
        inputs: &mut HashMap<UiKey, DisplayedInput>,
        buttons: &mut HashMap<UiKey, DisplayedButton>,
        entities: &[nara::tooling::SceneInspectorEntityRow],
    ) -> Result<Vec<UiNode>, OwnedInspectorError> {
        let Candidate::Structured(root) = &draft.candidate else {
            return Err(OwnedInspectorError::ChangedDraft);
        };
        let appearance = self.appearance;
        let typography: TextTypography = appearance.typography(UiTextRole::Body)?;
        let field_height = inspector_control_height(appearance, 36.0)?;
        let button_height = inspector_control_height(appearance, 30.0)?;
        let invalid = draft.error.is_some() || draft.incompatible;
        let rows = structure::rows(root, &draft.basis.field.shape);
        let control = self
            .controls
            .get_mut(key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        let owner = control.input_key().clone();
        let identity = control.identity;
        let enabled = control.allowed && control.host_enabled;
        let actions_enabled = enabled && !control.is_composing();
        let FieldInput::Structured(controls) = &mut control.input else {
            return Err(OwnedInspectorError::ReadOnlyField);
        };
        controls.synchronize(&rows);
        let mut nodes = Vec::new();
        for (index, row) in rows.into_iter().enumerate() {
            match row {
                StructuredRow::Label(label) => {
                    nodes.push(
                        UiNode::new(UiStyle::flex().with_flex_shrink(0.0))
                            .with_key(UiLocalKey::runtime(
                                "nara.inspector.structure-label",
                                format!("{identity}.{index}"),
                            )?)
                            .with_text(
                                appearance
                                    .text(label, UiTextRole::Caption, true)?
                                    .with_overflow(UiTextOverflow::Clip),
                            ),
                    );
                }
                StructuredRow::Text { input, label, text } => {
                    if !controls.inputs.contains_key(&input) {
                        match UiTextField::with_typography(
                            self.root.clone(),
                            &label,
                            &text,
                            typography,
                            &mut self.engine,
                        ) {
                            Ok(text) => {
                                controls.inputs.insert(input.clone(), Box::new(text));
                            }
                            Err(error) => {
                                nodes.push(
                                    UiNode::new(UiStyle::flex().with_flex_shrink(0.0))
                                        .with_key(UiLocalKey::runtime(
                                            "nara.inspector.structure-label",
                                            format!("{identity}.{index}"),
                                        )?)
                                        .with_text(
                                            appearance
                                                .text(
                                                    format!("Read-only: {error}"),
                                                    UiTextRole::Caption,
                                                    false,
                                                )?
                                                .with_brush(UiBrush::Solid(
                                                    appearance.palette().danger_text,
                                                ))
                                                .with_overflow(UiTextOverflow::Clip),
                                        ),
                                );
                                continue;
                            }
                        }
                    }
                    let text = controls
                        .inputs
                        .get_mut(&input)
                        .ok_or(OwnedInspectorError::RetiredField)?;
                    text.reconfigure(&mut self.engine, typography)?;
                    if !enabled {
                        text.blur(&mut self.engine)?;
                    }
                    text.set_enabled(enabled, &mut self.engine)?;
                    text.set_appearance(inspector_field_appearance(
                        appearance, text, width, invalid,
                    )?);
                    nodes.push(
                        text.authored_node(
                            &self.root,
                            UiStyle::flex()
                                .with_width(UiDimension::Px(width))
                                .with_height(UiDimension::Px(field_height))
                                .with_flex_shrink(0.0),
                            width,
                        )?
                        .with_semantics(
                            UiSemantics::new(UiSemanticRole::TextInput, label.clone())?
                                .with_value(text.editor().text())?,
                        ),
                    );
                    inputs.insert(
                        text.key().clone(),
                        DisplayedInput {
                            field: owner.clone(),
                            display_text: text.editor().display_text().to_owned(),
                            typography,
                            padding: text
                                .appearance()
                                .ok_or(OwnedInspectorError::ChangedProjection)?
                                .padding,
                        },
                    );
                    if structure::is_entity_reference(&draft.basis.field.shape, &input) {
                        let picker = controls.references.entry(input.clone()).or_default();
                        picker.appearance = appearance;
                        for (node, button) in references::reference_nodes(
                            &self.root,
                            &mut self.next_identity,
                            picker,
                            entities,
                            references::ReferenceContext {
                                field: &owner,
                                input: Some(&input),
                                name: Some(&label),
                                enabled: actions_enabled,
                                width,
                            },
                        )? {
                            nodes.push(node);
                            buttons.insert(button.button.key().clone(), button);
                        }
                    }
                }
                StructuredRow::Button {
                    action,
                    label,
                    name,
                    enabled: available,
                } => {
                    if !controls.buttons.contains_key(&action) {
                        let identity = self.next_identity;
                        self.next_identity = identity
                            .checked_add(1)
                            .ok_or(OwnedInspectorError::IdentityExhausted)?;
                        controls.buttons.insert(
                            action.clone(),
                            ButtonControl {
                                key: UiKey::scoped(
                                    self.root.clone(),
                                    UiLocalKey::runtime(
                                        "nara.inspector.structure-action",
                                        identity.to_string(),
                                    )?,
                                )?,
                                state: UiButtonState::default(),
                            },
                        );
                    }
                    let state = &controls.buttons[&action];
                    let button = UiButton::new(state.key.clone(), label, UiActionId::new(label)?)?
                        .with_enabled(actions_enabled && available);
                    let variant = match action.operation {
                        structure::StructureOperation::RemoveItem(_)
                        | structure::StructureOperation::RemoveEntry(_) => {
                            UiButtonVariant::Destructive
                        }
                        structure::StructureOperation::ConfirmSome
                        | structure::StructureOperation::ConfirmItem
                        | structure::StructureOperation::ConfirmEntry => UiButtonVariant::Primary,
                        _ => UiButtonVariant::Secondary,
                    };
                    nodes.push(
                        appearance
                            .button(&button, state.state, variant, UiControlSize::Regular, false)?
                            .authored_node(
                                &button,
                                &self.root,
                                UiStyle::flex()
                                    .with_width(UiDimension::Px(width))
                                    .with_height(UiDimension::Px(button_height))
                                    .with_flex_shrink(0.0),
                            )?
                            .with_semantics(
                                UiSemantics::new(UiSemanticRole::Button, name)?
                                    .with_action(button.action().clone())?,
                            ),
                    );
                    buttons.insert(
                        button.key().clone(),
                        DisplayedButton {
                            field: owner.clone(),
                            action: InspectorButtonAction::Structure(action),
                            button,
                        },
                    );
                }
            }
        }
        Ok(nodes)
    }

    pub(super) fn change_structure(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        button: &UiKey,
        action: &DraftAction,
        model: &EditorFrameModel,
        fields: &mut FieldEditors,
    ) -> Result<(), OwnedInspectorError> {
        let displayed = self.live_field(frame, field)?.clone();
        let key = &displayed.key;
        if !fields.has_draft_capacity(key) {
            return Err(OwnedInspectorError::DraftCapacity);
        }
        let (basis, editable) =
            current_basis(model, key).ok_or(OwnedInspectorError::ChangedProjection)?;
        let control = &self.controls[key];
        if self.read_only
            || !editable
            || !control.allowed
            || !control.host_enabled
            || displayed.current.as_ref() != Some(&basis)
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        let FieldInput::Structured(controls) = &control.input else {
            return Err(OwnedInspectorError::ReadOnlyField);
        };
        if controls
            .buttons
            .get(action)
            .is_none_or(|state| &state.key != button)
            || self
                .retained(frame)?
                .buttons
                .get(button)
                .is_none_or(|button| !button.button.enabled())
        {
            return Err(OwnedInspectorError::UnknownControl);
        }
        if control.is_composing() {
            return Err(OwnedInspectorError::Composing);
        }
        let draft = fields
            .drafts
            .get_mut(key)
            .ok_or(OwnedInspectorError::ChangedDraft)?;
        draft.synchronize(basis);
        if displayed.draft.as_ref() != Some(&DisplayedDraft::observe(draft)) {
            return Err(OwnedInspectorError::ChangedDraft);
        }
        if draft.incompatible || draft.in_flight() {
            return Err(OwnedInspectorError::Pending);
        }
        let Candidate::Structured(root) = &mut draft.candidate else {
            return Err(OwnedInspectorError::ChangedDraft);
        };
        structure::apply(
            root,
            &draft.basis.field.shape,
            source(&draft.basis.field),
            action,
        )
        .map_err(OwnedInspectorError::InvalidCandidate)?;
        let rows = structure::rows(root, &draft.basis.field.shape);
        let control = self
            .controls
            .get_mut(key)
            .ok_or(OwnedInspectorError::RetiredField)?;
        control.copy_feedback = None;
        if let FieldInput::Structured(controls) = &mut control.input {
            controls.synchronize(&rows);
        }
        draft.dirty = true;
        draft.remove_intent = false;
        draft.error = draft.candidate.value(&draft.basis.field.shape).err();
        Ok(())
    }
}
