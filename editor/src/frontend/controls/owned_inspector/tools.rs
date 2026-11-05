use nara::{
    reflect::ComponentPropertyPath,
    tooling::{
        EditorDocumentQueryBasis, EditorPropertyTarget, EditorSourceToolAction,
        EditorSourceToolActivationId, EditorSourceToolIntent, EditorSourceToolRequestId,
    },
};

use super::*;
use crate::frontend::controls::source_tool::OwnedSourceToolPresentation;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ToolBasis {
    activation: EditorSourceToolActivationId,
    target: EditorPropertyTarget,
    pub(super) label: String,
    pub(super) initial: i64,
}

impl ToolBasis {
    pub(super) fn capture(
        model: &EditorFrameModel,
        key: &FieldKey,
        current: &FieldBasis,
    ) -> Option<Self> {
        source(&current.field)?.as_i64()?;
        let active = model.source_tool().active.as_ref()?;
        let property = active.integer_property()?;
        let target = EditorPropertyTarget {
            basis: EditorDocumentQueryBasis {
                document: key.document,
                revision: current.target.base_revision(),
                schema: current.catalog.clone(),
            },
            entity: key.entity.clone(),
            component: key.component.clone(),
            property: ComponentPropertyPath::new(key.field.clone()),
        };
        active.matches_integer_property(&target).then(|| Self {
            activation: active.id().clone(),
            target,
            label: property.label().to_owned(),
            initial: property.initial(),
        })
    }
}

pub(super) struct ToolControl {
    pub(super) basis: ToolBasis,
    pub(super) submitting: bool,
    pub(super) buttons: [UiButtonState; 2],
    pub(super) error: Option<String>,
}

impl ToolControl {
    pub(super) fn new(basis: ToolBasis) -> Self {
        Self {
            basis,
            submitting: false,
            buttons: [UiButtonState::default(); 2],
            error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DisplayedTool {
    basis: ToolBasis,
    parameter: String,
    enabled: [bool; 2],
    review: Option<EditorSourceToolRequestId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::frontend) enum ToolAction {
    Prepare,
    Accept,
}

impl ToolAction {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Prepare => 0,
            Self::Accept => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Prepare => "Prepare property proposal",
            Self::Accept => "Accept reviewed proposal",
        }
    }
}

impl OwnedInspector {
    pub(in crate::frontend) fn retire_tool_controls(&mut self) {
        self.controls.retain(|_, control| control.tool.is_none());
    }

    pub(super) fn tool_nodes(
        &self,
        key: &FieldKey,
        width: f32,
        presentation: Option<&OwnedSourceToolPresentation>,
        model: &EditorFrameModel,
        buttons: &mut HashMap<UiKey, DisplayedButton>,
    ) -> Result<Option<(Vec<UiNode>, DisplayedTool)>, OwnedInspectorError> {
        let control = &self.controls[key];
        let Some(tool) = &control.tool else {
            return Ok(None);
        };
        let text = control.text().ok_or(OwnedInspectorError::ReadOnlyField)?;
        let parameter = text.editor().text().to_owned();
        let enabled =
            control.allowed && control.host_enabled && !tool.submitting && !control.is_composing();
        let mut displayed = DisplayedTool {
            basis: tool.basis.clone(),
            parameter: parameter.clone(),
            enabled: [enabled && parameter.parse::<i64>().is_ok(), false],
            review: None,
        };
        let identity = control.identity;
        let current = source(&control.basis.field)
            .and_then(nara::reflect::ComponentValue::as_i64)
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        let mut nodes = vec![self.caption(
            &format!("{identity}.tool.current"),
            &format!("Current value: {current}"),
        )?];
        if parameter.parse::<i64>().is_err() {
            nodes.push(self.error_label(
                &format!("{identity}.tool.invalid"),
                "Enter an i64 integer before preparing a proposal",
            )?);
        }
        if let Some(error) = &tool.error {
            nodes.push(self.error_label(&format!("{identity}.tool.error"), error)?);
        }
        let height = inspector_control_height(self.appearance, 30.0)?;
        for action in [ToolAction::Prepare, ToolAction::Accept] {
            if action == ToolAction::Accept {
                let Some(review) =
                    presentation.and_then(|presentation| presentation.review.as_ref())
                else {
                    continue;
                };
                nodes.push(self.caption(&format!("{identity}.tool.review"), "Source-tool review")?);
                nodes.push(self.label(&format!("{identity}.tool.title"), &review.title)?);
                for (index, operation) in review.operations.iter().enumerate() {
                    nodes.push(
                        self.label(&format!("{identity}.tool.operation.{index}"), operation)?,
                    );
                }
                let current = model.source_tool().review.as_ref().filter(|summary| {
                    summary.request == review.request && summary.basis == tool.basis.target.basis
                        && matches!(&summary.intent, EditorSourceToolIntent::Attach { entity } | EditorSourceToolIntent::Edit { entity }
                            if entity == &tool.basis.target.entity)
                });
                displayed.enabled[action.index()] = enabled && current.is_some();
                displayed.review = Some(review.request.clone());
                if current.is_none() {
                    nodes.push(self.error_label(&format!("{identity}.tool.stale"), "Review belongs to another source observation or entity; prepare a new proposal")?);
                }
            }
            let key = UiKey::scoped(
                self.root.clone(),
                UiLocalKey::runtime(
                    "nara.inspector.source-tool",
                    format!("{identity}.{}", action.index()),
                )?,
            )?;
            let button = UiButton::new(
                key.clone(),
                action.label(),
                UiActionId::new(action.label())?,
            )?
            .with_enabled(displayed.enabled[action.index()]);
            let variant = match action {
                ToolAction::Prepare => UiButtonVariant::Secondary,
                ToolAction::Accept => UiButtonVariant::Primary,
            };
            nodes.push(
                self.appearance
                    .button(
                        &button,
                        tool.buttons[action.index()],
                        variant,
                        UiControlSize::Regular,
                        false,
                    )?
                    .authored_node(
                        &button,
                        &self.root,
                        UiStyle::flex()
                            .with_width(UiDimension::Px(width))
                            .with_height(UiDimension::Px(height))
                            .with_flex_shrink(0.0),
                    )?,
            );
            buttons.insert(
                key,
                DisplayedButton {
                    field: control.input_key().clone(),
                    action: InspectorButtonAction::SourceTool(action),
                    button,
                },
            );
        }
        if let Some(presentation) = presentation {
            for (index, feedback) in presentation.feedback.iter().enumerate() {
                nodes.push(self.caption(&format!("{identity}.tool.feedback.{index}"), feedback)?);
            }
        }
        Ok(Some((nodes, displayed)))
    }

    pub(in crate::frontend) fn source_tool_action(
        &self,
        frame: &Arc<UiFrame>,
        button: &UiKey,
        model: &EditorFrameModel,
    ) -> Result<EditorSourceToolAction, OwnedInspectorError> {
        let binding = self
            .retained(frame)?
            .buttons
            .get(button)
            .ok_or(OwnedInspectorError::UnknownControl)?;
        let InspectorButtonAction::SourceTool(action) = binding.action else {
            return Err(OwnedInspectorError::UnknownControl);
        };
        let displayed = self.live_field(frame, &binding.field)?;
        let original = displayed
            .tool
            .as_ref()
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        let control = &self.controls[&displayed.key];
        let tool = control
            .tool
            .as_ref()
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        if tool.submitting || model.source_tool().working {
            return Err(OwnedInspectorError::Pending);
        }
        if self.read_only
            || !control.allowed
            || !control.host_enabled
            || !original.enabled[action.index()]
        {
            return Err(OwnedInspectorError::ReadOnlyField);
        }
        if control.is_composing() {
            return Err(OwnedInspectorError::Composing);
        }
        let (current, editable) =
            current_basis(model, &displayed.key).ok_or(OwnedInspectorError::ChangedProjection)?;
        if !editable
            || ToolBasis::capture(model, &displayed.key, &current).as_ref() != Some(&original.basis)
            || tool.basis != original.basis
        {
            return Err(OwnedInspectorError::ChangedProjection);
        }
        match action {
            ToolAction::Prepare => {
                let parameter = control
                    .text()
                    .ok_or(OwnedInspectorError::ReadOnlyField)?
                    .editor()
                    .text();
                if parameter != original.parameter {
                    return Err(OwnedInspectorError::ChangedDraft);
                }
                let value = parameter.parse::<i64>().map_err(|_| {
                    OwnedInspectorError::InvalidCandidate("Enter an i64 integer".into())
                })?;
                Ok(EditorSourceToolAction::IntegerProperty {
                    activation: original.basis.activation.clone(),
                    target: original.basis.target.clone(),
                    value,
                })
            }
            ToolAction::Accept => {
                let request = original
                    .review
                    .as_ref()
                    .ok_or(OwnedInspectorError::ChangedProjection)?;
                let current = model.source_tool();
                if current.request.as_ref() != Some(request) || current.review.as_ref().is_none_or(|summary| {
                    &summary.request != request || summary.basis != original.basis.target.basis
                        || !matches!(&summary.intent, EditorSourceToolIntent::Attach { entity } | EditorSourceToolIntent::Edit { entity }
                            if entity == &original.basis.target.entity)
                }) { return Err(OwnedInspectorError::ChangedProjection); }
                Ok(EditorSourceToolAction::Accept {
                    request: request.clone(),
                })
            }
        }
    }

    pub(in crate::frontend) fn tool_queued(
        &mut self,
        frame: &Arc<UiFrame>,
        input: &UiKey,
    ) -> Result<(), OwnedInspectorError> {
        let key = self.live_field(frame, input)?.key.clone();
        self.controls
            .get_mut(&key)
            .and_then(|control| control.tool.as_mut())
            .ok_or(OwnedInspectorError::ChangedProjection)?
            .submitting = true;
        Ok(())
    }

    pub(in crate::frontend) fn is_tool_input(&self, frame: &Arc<UiFrame>, input: &UiKey) -> bool {
        self.live_input(frame, input)
            .ok()
            .is_some_and(|displayed| self.controls[&displayed.key].tool.is_some())
    }

    pub(in crate::frontend) fn prepare_tool_input(
        &self,
        frame: &Arc<UiFrame>,
        input: &UiKey,
        model: &EditorFrameModel,
    ) -> Result<EditorSourceToolAction, OwnedInspectorError> {
        self.live_input(frame, input)?;
        let button = self
            .retained(frame)?
            .buttons
            .iter()
            .find(|(_, button)| {
                &button.field == input
                    && matches!(
                        button.action,
                        InspectorButtonAction::SourceTool(ToolAction::Prepare)
                    )
            })
            .map(|(key, _)| key)
            .ok_or(OwnedInspectorError::UnknownControl)?;
        self.source_tool_action(frame, button, model)
    }

    pub(in crate::frontend) fn discard_tool_input(
        &mut self,
        frame: &Arc<UiFrame>,
        input: &UiKey,
    ) -> Result<(), OwnedInspectorError> {
        let key = self.live_input(frame, input)?.key.clone();
        let tool = self.controls[&key]
            .tool
            .as_ref()
            .ok_or(OwnedInspectorError::ChangedProjection)?;
        if tool.submitting {
            return Err(OwnedInspectorError::Pending);
        }
        self.controls.remove(&key);
        Ok(())
    }
}
