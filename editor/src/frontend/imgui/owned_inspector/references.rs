use super::*;
use crate::frontend::imgui::field_drafts::references::entity_choices;
use nara::tooling::SceneInspectorEntityRow;

const TARGETS_PER_PAGE: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::frontend::imgui) enum ReferenceAction {
    Toggle,
    Previous,
    Next,
    Choose(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::scene::SceneEntityId;
    use nara_ui_kernel::{UiInputEvent, UiInteractionSession, UiRouteStamp, UiSeatId, UiSurfaceId};

    #[test]
    fn menu_view_replacement_preserves_focused_controls_and_cancels_pointer_state() {
        let root = UiRootKey::runtime("reference-focus", "panel").unwrap();
        let field = UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("reference-focus", "field").unwrap(),
        )
        .unwrap();
        let entities = (0..65)
            .map(|index| SceneInspectorEntityRow {
                id: SceneEntityId::new(format!("target-{index}")).unwrap(),
                name: None,
                placement: None,
                inspectable_component_count: 0,
                has_prefab: false,
                selected: false,
                live_locator: None,
            })
            .collect::<Vec<_>>();
        for action in [ReferenceAction::Toggle, ReferenceAction::Next] {
            let mut picker = ReferencePicker::default();
            if action == ReferenceAction::Next {
                picker.set_view(true, 0).unwrap();
            }
            let mut next_identity = 1;
            let nodes = reference_nodes(
                &root,
                &mut next_identity,
                &mut picker,
                &entities,
                ReferenceContext {
                    field: &field,
                    input: None,
                    name: None,
                    enabled: true,
                    width: 500.0,
                },
            )
            .unwrap();
            let button = nodes
                .iter()
                .find_map(|(_, button)| match &button.action {
                    InspectorButtonAction::Reference(binding) if binding.action == action => {
                        Some(button.button.clone())
                    }
                    _ => None,
                })
                .unwrap();
            let snapshot = UiAuthoringSnapshotBuilder::new(1)
                .with_root(
                    UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill())
                        .with_children(nodes.into_iter().map(|(node, _)| node)),
                )
                .build()
                .unwrap();
            let environment = UiEnvironment::new(
                UiSurfaceId::new(13).unwrap(),
                600.0,
                2000.0,
                1.0,
                1,
                1,
                1,
                1,
            )
            .unwrap();
            let mut compiler =
                UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
                    .unwrap();
            let frame = compiler
                .compile(&snapshot, environment, UiTheme::default())
                .unwrap();
            let (mut journal, authority) =
                nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
            journal
                .install_surface(environment.surface_id(), 1)
                .unwrap();
            let receipt = authority
                .submit(&mut journal, Arc::clone(&frame), 1)
                .unwrap();
            let mut session = UiInteractionSession::default();
            let dispatch = session
                .dispatch(
                    &journal,
                    UiRouteStamp::from_receipt(receipt),
                    UiInputEvent::FocusTarget {
                        seat: UiSeatId::PRIMARY,
                        target: button.key().clone(),
                    },
                )
                .unwrap();
            picker
                .buttons
                .get_mut(&action)
                .unwrap()
                .state
                .reduce_batch(
                    &button,
                    dispatch.consumed().unwrap(),
                    &mut UiIntentBatch::default(),
                )
                .unwrap();
            assert!(picker.buttons[&action].state.focused());
            let rect = frame
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(button.key()))
                .unwrap()
                .rect();
            let position = UiPoint::new(rect.origin().x() + 2.0, rect.origin().y() + 2.0);
            for event in [
                UiInputEvent::PointerMoved {
                    seat: UiSeatId::PRIMARY,
                    pointer: nara_ui_kernel::UiPointerId::PRIMARY,
                    position,
                },
                UiInputEvent::PointerButton {
                    seat: UiSeatId::PRIMARY,
                    pointer: nara_ui_kernel::UiPointerId::PRIMARY,
                    button: nara_ui_kernel::UiPointerButton::Primary,
                    phase: nara_ui_kernel::UiButtonPhase::Pressed,
                    position: Some(position),
                },
            ] {
                let dispatch = session
                    .dispatch(&journal, UiRouteStamp::from_receipt(receipt), event)
                    .unwrap();
                picker
                    .buttons
                    .get_mut(&action)
                    .unwrap()
                    .state
                    .reduce_batch(
                        &button,
                        dispatch.consumed().unwrap(),
                        &mut UiIntentBatch::default(),
                    )
                    .unwrap();
            }
            assert!(
                picker.buttons[&action].state.pressed() && picker.buttons[&action].state.hovered()
            );
            let old_revision = picker.revision;
            picker
                .set_view(true, usize::from(action == ReferenceAction::Next))
                .unwrap();
            let state = picker.buttons[&action].state;
            assert!(state.focused() && !state.pressed() && !state.hovered());
            assert_eq!(state.visual_state(&button), UiButtonVisualState::Focused);
            assert!(picker.revision > old_revision);
        }
    }

    #[test]
    fn navigation_before_repaint_updates_focus_but_refuses_old_menu_activation() {
        use nara::tooling::{EditorActionBatch, EditorNavigationAction};
        use nara_ui_kernel::UiActivationSource;
        use std::time::{Duration, Instant};
        let project = crate::frontend::imgui::interaction_tests::InteractionProject::new_nested_entity_references();
        let mut application = crate::EditorApplication::new(
            crate::EditorLaunchOptions::new(&project.root).with_profile("desktop"),
        );
        application.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while application.current_frame().is_none() && Instant::now() < deadline {
            application.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        let model = application.current_frame().unwrap();
        let document = model.workspace().active_document.unwrap();
        application.submit_and_drive(
            EditorActionBatch::new(
                model.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: Some(SceneEntityId::new("source").unwrap()),
                    },
                )],
            ),
            Duration::ZERO,
        );
        let model = application.current_frame().unwrap();
        let mut fields = FieldEditors::default();
        fields.begin_paint(&model);
        let mut inspector = OwnedInspector::new(
            UiRootKey::runtime("reference-navigation", "panel").unwrap(),
            vec![nara_text::bundled_font_face().unwrap()],
        )
        .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(23).unwrap(),
            600.0,
            4000.0,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        let frame = inspector
            .compile(
                &model,
                &mut fields,
                environment,
                UiTheme::default(),
                false,
                None,
            )
            .unwrap();
        let open = frame
            .semantics()
            .iter()
            .find(|node| node.name() == "root: Edit value")
            .unwrap()
            .key()
            .clone();
        inspector
            .activate(&frame, &open, &model, &mut fields)
            .unwrap();
        fields.begin_paint(&model);
        let frame = inspector
            .compile(
                &model,
                &mut fields,
                environment,
                UiTheme::default(),
                false,
                None,
            )
            .unwrap();
        let toggle = frame
            .semantics()
            .iter()
            .find(|node| node.name() == "root.direct: Choose target")
            .unwrap()
            .key()
            .clone();
        let input = frame
            .semantics()
            .iter()
            .find(|node| node.name() == "root.direct" && node.role() == UiSemanticRole::TextInput)
            .unwrap()
            .key()
            .clone();
        let InspectorBinding::Reference { input: target, .. } =
            inspector.binding(&frame, &toggle).unwrap()
        else {
            panic!("reference binding")
        };
        let InspectorBinding::Field(key) = inspector.binding(&frame, &input).unwrap() else {
            panic!("field binding")
        };
        let (mut journal, authority) =
            nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
        journal
            .install_surface(environment.surface_id(), 1)
            .unwrap();
        let route = UiRouteStamp::from_receipt(
            authority
                .submit(&mut journal, Arc::clone(&frame), 1)
                .unwrap(),
        );
        let mut session = UiInteractionSession::default();
        let focus = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::FocusTarget {
                    seat: UiSeatId::PRIMARY,
                    target: toggle.clone(),
                },
            )
            .unwrap();
        inspector
            .dispatch(&frame, focus.consumed().unwrap())
            .unwrap();
        assert!(
            inspector.controls[&key]
                .reference(target.as_ref())
                .unwrap()
                .buttons[&ReferenceAction::Toggle]
                .state
                .focused()
        );
        inspector
            .activate(&frame, &toggle, &model, &mut fields)
            .unwrap();
        let lost_focus = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::FocusTarget {
                    seat: UiSeatId::PRIMARY,
                    target: input.clone(),
                },
            )
            .unwrap();
        inspector
            .dispatch(&frame, lost_focus.consumed().unwrap())
            .unwrap();
        assert!(
            !inspector.controls[&key]
                .reference(target.as_ref())
                .unwrap()
                .buttons[&ReferenceAction::Toggle]
                .state
                .focused()
        );
        assert!(inspector.input_token(&frame, &input).unwrap().is_some());
        let focus = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::FocusTarget {
                    seat: UiSeatId::PRIMARY,
                    target: toggle.clone(),
                },
            )
            .unwrap();
        inspector
            .dispatch(&frame, focus.consumed().unwrap())
            .unwrap();
        let activation = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::ActivateFocused {
                    seat: UiSeatId::PRIMARY,
                    source: UiActivationSource::Keyboard,
                },
            )
            .unwrap();
        assert!(
            inspector
                .dispatch(&frame, activation.consumed().unwrap())
                .unwrap()
                .intents()
                .is_empty()
        );
        assert!(matches!(
            inspector.activate(&frame, &toggle, &model, &mut fields),
            Err(OwnedInspectorError::ChangedProjection)
        ));
    }
}

#[derive(Clone)]
pub(super) struct ReferenceBinding {
    pub(super) input: Option<DraftInput>,
    pub(super) revision: u64,
    pub(super) action: ReferenceAction,
}

#[derive(Default)]
pub(super) struct ReferencePicker {
    open: bool,
    page: usize,
    pub(super) revision: u64,
    pub(super) buttons: HashMap<ReferenceAction, ButtonControl>,
}

impl ReferencePicker {
    fn set_view(&mut self, open: bool, page: usize) -> Result<(), OwnedInspectorError> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(OwnedInspectorError::IdentityExhausted)?;
        self.open = open;
        self.page = page;
        self.revision = revision;
        self.buttons.retain(|action, _| match action {
            ReferenceAction::Toggle => true,
            ReferenceAction::Previous | ReferenceAction::Next => open,
            ReferenceAction::Choose(_) => false,
        });
        for control in self.buttons.values_mut() {
            control.state.cancel_pointer_interaction();
        }
        Ok(())
    }

    pub(super) fn cancel_input(&mut self) {
        for control in self.buttons.values_mut() {
            control.state = UiButtonState::default();
        }
    }
}

pub(super) struct ReferenceContext<'a> {
    pub(super) field: &'a UiKey,
    pub(super) input: Option<&'a DraftInput>,
    pub(super) name: Option<&'a str>,
    pub(super) enabled: bool,
    pub(super) width: f32,
}

pub(super) fn reference_nodes(
    root: &UiRootKey,
    next_identity: &mut u64,
    picker: &mut ReferencePicker,
    rows: &[SceneInspectorEntityRow],
    context: ReferenceContext<'_>,
) -> Result<Vec<(UiNode, DisplayedButton)>, OwnedInspectorError> {
    let mut descriptions = vec![(
        ReferenceAction::Toggle,
        if picker.open {
            "Hide targets".to_owned()
        } else {
            "Choose target".to_owned()
        },
        true,
    )];
    if picker.open {
        let choices = entity_choices(rows);
        let last_page = choices.len().saturating_sub(1) / TARGETS_PER_PAGE;
        if picker.page > last_page {
            picker.set_view(true, last_page)?;
        }
        descriptions.push((
            ReferenceAction::Previous,
            "Previous targets".to_owned(),
            picker.page > 0,
        ));
        descriptions.push((
            ReferenceAction::Next,
            "Next targets".to_owned(),
            picker.page < last_page,
        ));
        descriptions.extend(
            choices
                .skip(picker.page * TARGETS_PER_PAGE)
                .take(TARGETS_PER_PAGE)
                .map(|(entity, label)| (ReferenceAction::Choose(entity.to_owned()), label, true)),
        );
    }
    picker
        .buttons
        .retain(|action, _| descriptions.iter().any(|(current, _, _)| current == action));
    descriptions
        .into_iter()
        .map(|(action, label, available)| {
            if !picker.buttons.contains_key(&action) {
                let identity = *next_identity;
                *next_identity = identity
                    .checked_add(1)
                    .ok_or(OwnedInspectorError::IdentityExhausted)?;
                picker.buttons.insert(
                    action.clone(),
                    ButtonControl {
                        key: UiKey::scoped(
                            root.clone(),
                            UiLocalKey::runtime("nara.inspector.reference", identity.to_string())?,
                        )?,
                        state: UiButtonState::default(),
                    },
                );
            }
            let control = &picker.buttons[&action];
            let name = context
                .name
                .map_or_else(|| label.clone(), |name| format!("{name}: {label}"));
            let button = UiButton::new(
                control.key.clone(),
                label,
                UiActionId::new("Reference control")?,
            )?
            .with_enabled(context.enabled && available);
            let node = button
                .authored_node(
                    root,
                    UiStyle::flex()
                        .with_width(UiDimension::Px(context.width))
                        .with_height(UiDimension::Px(30.0))
                        .with_flex_shrink(0.0),
                    UiVisual::role(button_color(control.state, &button)),
                    UiBrush::Role(UiColorRole::PrimaryText),
                    TEXT_SIZE,
                )?
                .with_semantics(
                    UiSemantics::new(UiSemanticRole::Button, name)?
                        .with_action(button.action().clone())?,
                );
            Ok((
                node,
                DisplayedButton {
                    field: context.field.clone(),
                    action: InspectorButtonAction::Reference(ReferenceBinding {
                        input: context.input.cloned(),
                        revision: picker.revision,
                        action,
                    }),
                    button,
                },
            ))
        })
        .collect()
}

impl OwnedInspector {
    pub(super) fn reference_action(
        &mut self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        displayed_button: &DisplayedButton,
        model: &EditorFrameModel,
        fields: &mut FieldEditors,
    ) -> Result<(), OwnedInspectorError> {
        let InspectorButtonAction::Reference(binding) = &displayed_button.action else {
            return Err(OwnedInspectorError::UnknownControl);
        };
        let displayed = self.live_field(frame, &displayed_button.field)?.clone();
        let key = &displayed.key;
        let control = &self.controls[key];
        let input = match &binding.input {
            None => control.text(),
            Some(target) => match &control.input {
                FieldInput::Structured(controls) => controls.inputs.get(target).map(Box::as_ref),
                _ => None,
            },
        }
        .ok_or(OwnedInspectorError::RetiredField)?
        .key()
        .clone();
        self.check_input(key, &input, fields)?;
        let picker = control
            .reference(binding.input.as_ref())
            .ok_or(OwnedInspectorError::RetiredField)?;
        if picker.revision != binding.revision
            || picker
                .buttons
                .get(&binding.action)
                .is_none_or(|control| &control.key != button)
            || !displayed_button.button.enabled()
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        if control.is_composing() {
            return Err(OwnedInspectorError::Composing);
        }
        let (current, editable) =
            current_basis(model, key).ok_or(OwnedInspectorError::ChangedProjection)?;
        let reference_shape = binding.input.as_ref().map_or(
            current.field.shape == ComponentValueShape::EntityRef,
            |input| structure::is_entity_reference(&current.field.shape, input),
        );
        if !editable
            || self.read_only
            || !reference_shape
            || displayed.current.as_ref() != Some(&current)
            || displayed.draft.as_ref() != Some(&DisplayedDraft::observe(&fields.drafts[key]))
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        if let ReferenceAction::Choose(entity) = &binding.action {
            if !picker.open
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
            Self::adopt_text(
                fields,
                key,
                binding.input.as_ref(),
                &format!("scene-local:{entity}"),
            )?;
            if let Some(target) = &binding.input {
                let control = self
                    .controls
                    .get_mut(key)
                    .ok_or(OwnedInspectorError::RetiredField)?;
                control.copy_feedback = None;
                if let FieldInput::Structured(controls) = &mut control.input {
                    controls.inputs.remove(target);
                    controls.references.remove(target);
                }
            } else {
                self.controls.remove(key);
            }
            return Ok(());
        }
        let (open, page) = match binding.action {
            ReferenceAction::Toggle => (!picker.open, 0),
            ReferenceAction::Previous if picker.open && picker.page > 0 => (true, picker.page - 1),
            ReferenceAction::Next if picker.open => (
                true,
                picker
                    .page
                    .checked_add(1)
                    .ok_or(OwnedInspectorError::IdentityExhausted)?,
            ),
            _ => return Err(OwnedInspectorError::ChangedProjection),
        };
        self.controls
            .get_mut(key)
            .and_then(|control| control.reference_mut(binding.input.as_ref()))
            .ok_or(OwnedInspectorError::RetiredField)?
            .set_view(open, page)
    }
}
