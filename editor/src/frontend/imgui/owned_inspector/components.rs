use nara::{
    reflect::{ComponentSchemaVersion, ComponentTypeId},
    scene::SceneEntityId,
    tooling::{
        EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis,
        SceneCompositionCapabilities, SceneCompositionCommand, SceneInspectorComponentAccess,
        SceneInspectorComponentView,
    },
};

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ComponentAction {
    Menu,
    Add(ComponentTypeId),
    Fold(ComponentTypeId),
    Remove(ComponentTypeId),
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct CompositionBasis {
    target: EditorDocumentQueryBasis,
    kind: EditorDocumentKind,
    entity: SceneEntityId,
    editing: bool,
    capabilities: SceneCompositionCapabilities,
    components: Vec<(
        ComponentTypeId,
        ComponentSchemaVersion,
        ComponentSchemaVersion,
        SceneInspectorComponentAccess,
    )>,
}

impl CompositionBasis {
    fn capture(model: &EditorFrameModel, read_only: bool) -> Option<Self> {
        let scene = model.workspace().active_scene.as_ref()?;
        let entity = scene.editor.inspector.selected_entity_view.as_ref()?;
        Some(Self {
            target: EditorDocumentQueryBasis {
                document: scene.document,
                revision: scene.revision,
                schema: model
                    .workspace()
                    .document(scene.document)
                    .and_then(|document| document.authoring_schema_basis.clone()),
            },
            kind: scene.kind,
            entity: entity.id.clone(),
            editing: !read_only && authoring::editing_enabled(model),
            capabilities: scene.editor.inspector.composition.clone(),
            components: entity
                .components
                .iter()
                .map(|component| {
                    (
                        component.component.clone(),
                        component.document_version,
                        component.schema_version,
                        component.access,
                    )
                })
                .collect(),
        })
    }
}

#[derive(Default)]
pub(super) struct ComponentControls {
    selection: Option<(EditorDocumentId, SceneEntityId)>,
    menu: bool,
    collapsed: HashSet<ComponentTypeId>,
    revision: u64,
    buttons: HashMap<ComponentAction, ButtonControl>,
    error: Option<String>,
}

pub(in crate::frontend::imgui) struct ComponentSubmission {
    basis: CompositionBasis,
    action: EditorAction,
}

impl ComponentSubmission {
    pub(in crate::frontend::imgui) fn into_action(
        self,
        model: &EditorFrameModel,
        read_only: bool,
    ) -> Result<EditorAction, OwnedInspectorError> {
        if CompositionBasis::capture(model, read_only).as_ref() != Some(&self.basis) {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        Ok(self.action)
    }
}

#[derive(Clone)]
pub(super) struct DisplayedComposition {
    pub(super) basis: CompositionBasis,
    revision: u64,
    buttons: HashMap<UiKey, (ComponentAction, UiButton)>,
}

impl DisplayedComposition {
    pub(super) fn observation(&self) -> (CompositionBasis, u64) {
        (self.basis.clone(), self.revision)
    }

    pub(super) fn contains(&self, key: &UiKey) -> bool {
        self.buttons.contains_key(key)
    }

    pub(super) fn retain(&mut self, semantics: &HashMap<&UiKey, bool>) {
        self.buttons.retain(|key, _| semantics.contains_key(key));
    }
}

impl OwnedInspector {
    pub(super) fn begin_components(
        &mut self,
        model: &EditorFrameModel,
        width: f32,
        nodes: &mut Vec<UiNode>,
    ) -> Result<Option<DisplayedComposition>, OwnedInspectorError> {
        let Some(basis) = CompositionBasis::capture(model, self.read_only) else {
            self.components = ComponentControls::default();
            return Ok(None);
        };
        let selection = (basis.target.document, basis.entity.clone());
        if self.components.selection.as_ref() != Some(&selection) {
            self.components = ComponentControls {
                selection: Some(selection),
                ..Default::default()
            };
        }
        let present = basis
            .components
            .iter()
            .map(|(component, ..)| component.clone())
            .collect::<HashSet<_>>();
        self.components
            .collapsed
            .retain(|component| present.contains(component));
        let mut displayed = DisplayedComposition {
            basis,
            revision: self.components.revision,
            buttons: HashMap::new(),
        };
        if let Some(error) = &self.components.error {
            nodes.push(self.label("composition.error", error)?);
        }
        nodes.push(self.component_button(
            ComponentAction::Menu,
            "Add Component",
            true,
            width,
            &mut displayed,
        )?);
        if self.components.menu {
            for capability in displayed.basis.capabilities.components.clone() {
                let reason = if present.contains(&capability.component) {
                    Some("Component already exists")
                } else {
                    capability.add_unavailable_reason
                };
                let label = format!("Add {}", capability.component.as_str());
                nodes.push(self.component_button(
                    ComponentAction::Add(capability.component),
                    &label,
                    displayed.basis.editing && reason.is_none(),
                    width,
                    &mut displayed,
                )?);
                if let Some(reason) = reason {
                    nodes.push(self.label(&format!("{label}.unavailable"), reason)?);
                }
            }
        }
        Ok(Some(displayed))
    }

    pub(super) fn component_header(
        &mut self,
        component: &SceneInspectorComponentView,
        width: f32,
        nodes: &mut Vec<UiNode>,
        displayed: &mut DisplayedComposition,
    ) -> Result<bool, OwnedInspectorError> {
        let expanded = !self.components.collapsed.contains(&component.component);
        let name = component.component.as_str();
        nodes.push(self.component_button(
            ComponentAction::Fold(component.component.clone()),
            &format!("{} {name}", if expanded { "Collapse" } else { "Expand" }),
            true,
            width,
            displayed,
        )?);
        if !expanded {
            return Ok(false);
        }
        if let Some(reason) = component.access.read_only_reason() {
            nodes.push(self.label(
                &format!("{name}.read-only"),
                &format!("Read-only component: {reason:?}"),
            )?);
        }
        let removable = displayed.basis.editing
            && component.access.is_editable()
            && component.document_version == component.schema_version
            && displayed
                .basis
                .capabilities
                .components
                .iter()
                .any(|capability| {
                    capability.component == component.component && capability.removable
                });
        nodes.push(self.component_button(
            ComponentAction::Remove(component.component.clone()),
            &format!("Remove {name}"),
            removable,
            width,
            displayed,
        )?);
        if component.fields.is_empty() {
            nodes.push(self.label(&format!("{name}.empty"), "No inspectable fields")?);
        }
        Ok(true)
    }

    fn component_button(
        &mut self,
        action: ComponentAction,
        label: &str,
        enabled: bool,
        width: f32,
        displayed: &mut DisplayedComposition,
    ) -> Result<UiNode, OwnedInspectorError> {
        if !self.components.buttons.contains_key(&action) {
            let identity = self.next_identity;
            self.next_identity = identity
                .checked_add(1)
                .ok_or(OwnedInspectorError::IdentityExhausted)?;
            let key = UiKey::scoped(
                self.root.clone(),
                UiLocalKey::runtime("nara.inspector.composition", identity.to_string())?,
            )?;
            self.components.buttons.insert(
                action.clone(),
                ButtonControl {
                    key,
                    state: UiButtonState::default(),
                },
            );
        }
        let control = &self.components.buttons[&action];
        let button = UiButton::new(control.key.clone(), label, UiActionId::new(label)?)?
            .with_enabled(enabled);
        let node = button.authored_node(
            &self.root,
            UiStyle::flex()
                .with_width(UiDimension::Px(width))
                .with_height(UiDimension::Px(30.0))
                .with_flex_shrink(0.0),
            UiVisual::role(button_color(control.state, &button)),
            UiBrush::Role(UiColorRole::PrimaryText),
            TEXT_SIZE,
        )?;
        displayed
            .buttons
            .insert(control.key.clone(), (action, button));
        Ok(node)
    }

    pub(super) fn invalidated_component_controls(
        previous: Option<&DisplayedComposition>,
        candidate: Option<&DisplayedComposition>,
    ) -> Vec<UiKey> {
        let Some(previous) = previous else {
            return Vec::new();
        };
        previous
            .buttons
            .keys()
            .filter(|key| {
                candidate.is_none_or(|candidate| {
                    previous.basis != candidate.basis
                        || previous.revision != candidate.revision
                        || !candidate.buttons.contains_key(*key)
                })
            })
            .cloned()
            .collect()
    }

    pub(super) fn finish_components(&mut self, displayed: Option<&DisplayedComposition>) {
        self.components.buttons.retain(|_, control| {
            displayed.is_some_and(|displayed| displayed.contains(&control.key))
        });
    }

    fn live_component(
        &self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
    ) -> Result<&DisplayedComposition, OwnedInspectorError> {
        let displayed = self
            .retained(frame)?
            .composition
            .as_ref()
            .filter(|displayed| displayed.contains(key))
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let (action, _) = &displayed.buttons[key];
        if self
            .components
            .buttons
            .get(action)
            .is_none_or(|control| control.key != *key)
            || self
                .frames
                .last()
                .and_then(|compiled| compiled.composition.as_ref())
                .is_none_or(|current| {
                    current.basis != displayed.basis
                        || current.revision != displayed.revision
                        || !current.contains(key)
                })
            || self.components.revision != displayed.revision
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        Ok(displayed)
    }

    pub(super) fn dispatch_components(
        &mut self,
        frame: &Arc<UiFrame>,
        dispatch: &UiDispatchBatch,
        intents: &mut UiIntentBatch,
    ) -> Result<(), OwnedInspectorError> {
        let buttons = self
            .retained(frame)?
            .composition
            .as_ref()
            .map(|displayed| displayed.buttons.clone())
            .unwrap_or_default();
        for (key, (action, button)) in buttons {
            let button = if self.live_component(frame, &key).is_ok() {
                button
            } else {
                button.with_enabled(false)
            };
            if let Some(control) = self.components.buttons.get_mut(&action)
                && control.key == key
            {
                control.state.reduce_batch(&button, dispatch, intents)?;
            }
        }
        Ok(())
    }

    pub(in crate::frontend::imgui) fn activate_component(
        &mut self,
        frame: &Arc<UiFrame>,
        key: &UiKey,
        model: &EditorFrameModel,
    ) -> Result<Option<ComponentSubmission>, OwnedInspectorError> {
        let displayed = self.live_component(frame, key)?;
        if CompositionBasis::capture(model, self.read_only).as_ref() != Some(&displayed.basis) {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        let (action, button) = &displayed.buttons[key];
        if !button.enabled() {
            return Err(OwnedInspectorError::CompositionDisabled);
        }
        let entity = displayed.basis.entity.clone();
        let basis = displayed.basis.clone();
        let close_menu = matches!(action, ComponentAction::Add(_));
        let command = match action {
            ComponentAction::Menu | ComponentAction::Fold(_) => {
                let revision = self
                    .components
                    .revision
                    .checked_add(1)
                    .ok_or(OwnedInspectorError::IdentityExhausted)?;
                match action {
                    ComponentAction::Menu => self.components.menu = !self.components.menu,
                    ComponentAction::Fold(component) => {
                        let component = component.clone();
                        if !self.components.collapsed.remove(&component) {
                            self.components.collapsed.insert(component);
                        }
                    }
                    _ => unreachable!(),
                }
                self.components.revision = revision;
                for control in self.components.buttons.values_mut() {
                    control.state.cancel_pointer_interaction();
                }
                return Ok(None);
            }
            ComponentAction::Add(component) => SceneCompositionCommand::AddComponent {
                entity,
                component: component.clone(),
            },
            ComponentAction::Remove(component) => SceneCompositionCommand::RemoveComponent {
                entity,
                component: component.clone(),
            },
        };
        let scene = model
            .workspace()
            .active_scene
            .as_ref()
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        let action = authoring::compose_action(scene, command);
        let revision = self
            .components
            .revision
            .checked_add(1)
            .ok_or(OwnedInspectorError::IdentityExhausted)?;
        self.components.revision = revision;
        self.components.error = None;
        for control in self.components.buttons.values_mut() {
            control.state.cancel_pointer_interaction();
        }
        if close_menu {
            self.components.menu = false;
            self.components.buttons.retain(|action, control| {
                control.state.cancel_pointer_interaction();
                !matches!(action, ComponentAction::Add(_))
            });
        }
        Ok(Some(ComponentSubmission { basis, action }))
    }

    pub(in crate::frontend::imgui) fn component_submission_failed(&mut self, error: String) {
        self.components.error = Some(error);
    }

    pub(super) fn cancel_component_input(&mut self) {
        for control in self.components.buttons.values_mut() {
            control.state = UiButtonState::default();
        }
    }
}
