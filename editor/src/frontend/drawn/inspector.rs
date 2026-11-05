//! Drawn-panel integration for the complete shared Inspector and its original field drafts.

use std::{collections::BTreeSet, sync::Arc};

use nara::tooling::{
    EditorActionOutcome, EditorSourceToolAction, EditorSourceToolActionOutcome,
    EditorSourceToolError,
};
use nara_text::{FontFace, TextEdit, TextEditOutcome};
use nara_ui_kernel::{
    UiDispatchBatch, UiEnvironment, UiFrame, UiKey, UiPoint, UiRect, UiRootKey, UiTheme,
};
use nara_ui_widgets::{UiAppearance, UiTextInputToken};

use crate::{
    EditorPanelContext, PanelActionDispatch, PanelSubmissionId, native::EditorPanelServices,
};

use super::super::controls::{
    editing_enabled,
    field_drafts::{FieldEditors, PreparedFieldEdit},
    owned_inspector::{InspectorBinding, InspectorOperation, OwnedInspector, OwnedInspectorError},
    source_tool::OwnedSourceToolPresentation,
};

const MAX_PENDING_COPIES: usize = 64;
const MAX_FEEDBACK_CHARS: usize = 1024;

struct PendingCopy {
    frame: Arc<UiFrame>,
    button: UiKey,
    text: String,
}

/// Each panel retains the original editing owners; the Session retains source-tool proposals.
pub(super) struct InspectorControls {
    inspector: OwnedInspector,
    fields: FieldEditors,
    component_pending: BTreeSet<PanelSubmissionId>,
    source_tool_pending: Option<PanelSubmissionId>,
    source_tool_feedback: Option<String>,
    pending_copies: Vec<PendingCopy>,
    appearance: UiAppearance,
}

impl InspectorControls {
    pub(super) fn new(root: UiRootKey, fonts: Vec<FontFace>) -> Result<Self, String> {
        Ok(Self {
            inspector: OwnedInspector::new(root, fonts).map_err(|error| error.to_string())?,
            fields: FieldEditors::default(),
            component_pending: BTreeSet::new(),
            source_tool_pending: None,
            source_tool_feedback: None,
            pending_copies: Vec::new(),
            appearance: UiAppearance::default(),
        })
    }

    pub(super) fn set_appearance(&mut self, appearance: UiAppearance) {
        self.appearance = appearance;
        self.inspector.set_appearance(appearance);
    }

    pub(super) fn compile(
        &mut self,
        context: &mut EditorPanelContext<'_, '_>,
        environment: UiEnvironment,
    ) -> Result<Arc<UiFrame>, String> {
        let model = context.model().frame();
        self.fields.begin_paint(model);
        let review = context.source_tool_review().ok().flatten();
        let mut feedback = Vec::new();
        if let Some(message) = &self.source_tool_feedback {
            feedback.push(bounded(message));
        }
        if self.source_tool_pending.is_some() {
            feedback.push("Source-tool action queued".to_owned());
        }
        let tools = OwnedSourceToolPresentation::capture(
            model,
            review.as_ref(),
            editing_enabled(model) && self.source_tool_pending.is_none(),
            feedback,
        );
        self.inspector
            .compile(
                model,
                &mut self.fields,
                environment,
                UiTheme::new(
                    environment.theme_revision().get(),
                    self.appearance.palette().kernel_palette(),
                )
                .map_err(|error| error.to_string())?,
                false,
                Some(&tools),
            )
            .map_err(|error| error.to_string())
    }

    pub(super) fn reduce(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
        context: &mut EditorPanelContext<'_, '_>,
    ) -> Result<(), String> {
        let intents = self
            .inspector
            .dispatch(frame, batch)
            .map_err(|error| error.to_string())?;
        for intent in intents.intents() {
            match self
                .inspector
                .binding(frame, intent.target())
                .map_err(|error| error.to_string())?
            {
                InspectorBinding::Composition => {
                    let submission = match self.inspector.activate_component(
                        frame,
                        intent.target(),
                        context.model().frame(),
                    ) {
                        Ok(submission) => submission,
                        Err(error) => {
                            self.activation_refused(frame, None, error)?;
                            continue;
                        }
                    };
                    if let Some(submission) = submission {
                        let action = match submission.into_action(context.model().frame(), false) {
                            Ok(action) => action,
                            Err(error) => {
                                self.activation_refused(frame, None, error)?;
                                continue;
                            }
                        };
                        match context.submit(action) {
                            Ok(id) => {
                                self.component_pending.insert(id);
                            }
                            Err(error) => self.inspector.component_submission_failed(format!(
                                "Component action was not queued: {error}. Review and retry."
                            )),
                        }
                    }
                }
                InspectorBinding::Copy { field } => {
                    if self.pending_copies.len() >= MAX_PENDING_COPIES {
                        self.inspector
                            .copy_finished(
                                frame,
                                intent.target(),
                                Err("Inspector clipboard request capacity reached".to_owned()),
                            )
                            .map_err(|error| error.to_string())?;
                        continue;
                    }
                    let text = match self
                        .inspector
                        .copy_draft(frame, intent.target(), &self.fields)
                    {
                        Ok(text) => text,
                        Err(error) => {
                            self.activation_refused(frame, Some(&field), error)?;
                            continue;
                        }
                    };
                    self.pending_copies.push(PendingCopy {
                        frame: Arc::clone(frame),
                        button: intent.target().clone(),
                        text,
                    });
                }
                InspectorBinding::SourceTool { field, .. } => {
                    let action = match self.inspector.source_tool_action(
                        frame,
                        intent.target(),
                        context.model().frame(),
                    ) {
                        Ok(action) => action,
                        Err(error) => {
                            self.activation_refused(frame, Some(&field), error)?;
                            continue;
                        }
                    };
                    self.submit_source_tool(frame, &field, action, context)?;
                }
                binding @ (InspectorBinding::DiscardRetainedDrafts
                | InspectorBinding::Operation { .. }
                | InspectorBinding::Reference { .. }
                | InspectorBinding::Structure { .. }) => {
                    let field = match binding {
                        InspectorBinding::Operation { field, .. }
                        | InspectorBinding::Reference { field, .. }
                        | InspectorBinding::Structure { field, .. } => Some(field),
                        _ => None,
                    };
                    match self.inspector.activate(
                        frame,
                        intent.target(),
                        context.model().frame(),
                        &mut self.fields,
                    ) {
                        Ok(Some(prepared)) => self.submit_field(prepared, context),
                        Ok(None) => {}
                        Err(error) => self.activation_refused(frame, field.as_ref(), error)?,
                    }
                }
                InspectorBinding::Field(_) => {}
            }
        }
        Ok(())
    }

    fn activation_refused(
        &mut self,
        frame: &Arc<UiFrame>,
        field: Option<&UiKey>,
        error: OwnedInspectorError,
    ) -> Result<(), String> {
        if !matches!(
            error,
            OwnedInspectorError::ChangedDraft
                | OwnedInspectorError::ChangedProjection
                | OwnedInspectorError::ReadOnlyField
                | OwnedInspectorError::CompositionDisabled
                | OwnedInspectorError::DraftCapacity
                | OwnedInspectorError::Pending
                | OwnedInspectorError::Composing
                | OwnedInspectorError::OperationDisabled(_)
                | OwnedInspectorError::InvalidCandidate(_)
        ) {
            return Err(error.to_string());
        }
        let message = format!(
            "Action was not applied: {}. Review and retry.",
            bounded(&error)
        );
        if let Some(field) = field {
            match self.inspector.input_token(frame, field) {
                Ok(Some(token)) => {
                    self.inspector
                        .input_error(frame, field, token, &mut self.fields, message);
                    return Ok(());
                }
                Ok(None)
                | Err(
                    OwnedInspectorError::ReadOnlyField | OwnedInspectorError::ChangedProjection,
                ) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        self.inspector.component_submission_failed(message);
        Ok(())
    }

    fn submit_field(
        &mut self,
        prepared: PreparedFieldEdit,
        context: &mut EditorPanelContext<'_, '_>,
    ) {
        let result = context.submit(prepared.action);
        self.fields.admitted(&prepared.key, result);
    }

    fn submit_source_tool(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        action: EditorSourceToolAction,
        context: &mut EditorPanelContext<'_, '_>,
    ) -> Result<(), String> {
        if self.source_tool_pending.is_some() {
            return Ok(());
        }
        if let Err(error) = self.inspector.tool_queued(frame, field) {
            return self.activation_refused(frame, Some(field), error);
        }
        match context.submit_source_tool(action) {
            Ok(id) => {
                self.source_tool_pending = Some(id);
                self.source_tool_feedback = None;
            }
            Err(error) => {
                self.source_tool_feedback = Some(format!(
                    "Source-tool action was not queued: {}. Review and try again.",
                    bounded(&error),
                ));
            }
        }
        Ok(())
    }

    /// Called only with the Host's services after actual input, never during reconciliation.
    pub(super) fn flush_services(
        &mut self,
        services: &mut EditorPanelServices<'_>,
    ) -> Result<(), String> {
        let mut failure = None;
        for request in std::mem::take(&mut self.pending_copies) {
            let result = services
                .write_clipboard(&request.text)
                .map_err(|error| error.to_string());
            if let Err(error) =
                self.inspector
                    .copy_finished(&request.frame, &request.button, result)
            {
                failure.get_or_insert_with(|| error.to_string());
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        self.fields.action_finished(id, result);
        if self.component_pending.remove(&id) {
            let message = match result {
                PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
                    if report.applied => None,
                PanelActionDispatch::NotDispatched(reason) => Some(format!(
                    "Component action was not dispatched: {reason:?}. Review and retry."
                )),
                PanelActionDispatch::Dispatched(_) => Some(
                    "Component action was not applied. Review the current source and Diagnostics before retrying."
                        .to_owned(),
                ),
            };
            if let Some(message) = message {
                self.inspector.component_submission_failed(message);
            }
            return;
        }
        if self.source_tool_pending != Some(id) {
            return;
        }
        self.source_tool_pending = None;
        let outcome = match result {
            PanelActionDispatch::Dispatched(EditorActionOutcome::SourceTool(outcome)) => outcome,
            PanelActionDispatch::NotDispatched(reason) => {
                self.source_tool_feedback = Some(format!(
                    "Source-tool action was not dispatched: {reason:?}. Review the current source before trying again."
                ));
                return;
            }
            PanelActionDispatch::Dispatched(_) => {
                self.source_tool_feedback = Some("Source-tool action was rejected by the Editor. Review the current source and Diagnostics before trying again.".to_owned());
                return;
            }
        };
        let error = match outcome {
            EditorSourceToolActionOutcome::Selection(result) => {
                self.source_tool_feedback = Some(match result {
                    Ok(_) => "Installed selection requested. Observe its durable result separately from physical retirement.".to_owned(),
                    Err(error) => format!("Installed selection: {}", bounded(error)),
                });
                return;
            }
            EditorSourceToolActionOutcome::Requested(Ok(_)) => {
                self.source_tool_feedback =
                    Some("Proposal requested. Waiting for the Session's review.".to_owned());
                return;
            }
            EditorSourceToolActionOutcome::Accepted(Ok(commit)) => {
                self.inspector.retire_tool_controls();
                self.source_tool_feedback = Some(if commit.report.applied {
                    "Reviewed source-tool proposal committed. Use ordinary Undo to revert it."
                        .to_owned()
                } else {
                    "The source owner did not apply the reviewed proposal. Check Diagnostics."
                        .to_owned()
                });
                return;
            }
            EditorSourceToolActionOutcome::Disabled(Ok(())) => {
                self.inspector.retire_tool_controls();
                self.source_tool_feedback = Some("Source tool disabled.".to_owned());
                return;
            }
            EditorSourceToolActionOutcome::Cancelled(Ok(())) => {
                self.source_tool_feedback = Some(
                    "Source-tool probe cancelled. Physical work may still be returning.".to_owned(),
                );
                return;
            }
            EditorSourceToolActionOutcome::Requested(Err(error))
            | EditorSourceToolActionOutcome::Accepted(Err(error))
            | EditorSourceToolActionOutcome::Disabled(Err(error))
            | EditorSourceToolActionOutcome::Cancelled(Err(error)) => error,
        };
        if matches!(
            error,
            EditorSourceToolError::StaleActivation
                | EditorSourceToolError::Disabled
                | EditorSourceToolError::QueryBasis(_)
                | EditorSourceToolError::Schema
                | EditorSourceToolError::PropertyFacet
                | EditorSourceToolError::StaleRequest
        ) {
            self.inspector.retire_tool_controls();
        }
        self.source_tool_feedback =
            Some(format!("Source-tool action rejected: {}", bounded(error)));
    }

    pub(super) fn finish_input(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        discard: bool,
        context: &mut EditorPanelContext<'_, '_>,
    ) -> Result<(), String> {
        if self.inspector.is_tool_input(frame, field) {
            if discard {
                return match self.inspector.discard_tool_input(frame, field) {
                    Ok(()) => Ok(()),
                    Err(error) => self.activation_refused(frame, Some(field), error),
                };
            }
            let action =
                match self
                    .inspector
                    .prepare_tool_input(frame, field, context.model().frame())
                {
                    Ok(action) => action,
                    Err(error) => return self.activation_refused(frame, Some(field), error),
                };
            return self.submit_source_tool(frame, field, action, context);
        }
        let operation = if discard {
            InspectorOperation::Discard
        } else {
            InspectorOperation::Apply
        };
        match self.inspector.operate(
            frame,
            field,
            operation,
            context.model().frame(),
            &mut self.fields,
        ) {
            Ok(Some(prepared)) => self.submit_field(prepared, context),
            Ok(None) => {}
            Err(error) => self.activation_refused(frame, Some(field), error)?,
        }
        Ok(())
    }

    pub(super) fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        self.inspector.retains_frame(frame)
    }

    pub(super) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, String> {
        self.inspector
            .invalidated_controls(previous, candidate)
            .map_err(|error| error.to_string())
    }

    pub(super) fn scroll(&mut self, frame: &Arc<UiFrame>, delta: f32) -> Result<bool, String> {
        self.inspector
            .scroll(frame, delta)
            .map_err(|error| error.to_string())
    }

    pub(super) fn input_token(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
    ) -> Result<Option<UiTextInputToken>, String> {
        self.inspector
            .input_token(frame, field)
            .map_err(|error| error.to_string())
    }

    pub(super) fn edit(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        edit: &TextEdit,
    ) -> Result<TextEditOutcome, String> {
        self.inspector
            .edit(frame, field, token, edit, &mut self.fields)
            .map_err(|error| error.to_string())
    }

    pub(super) fn point(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        position: UiPoint,
        extend: bool,
    ) -> Result<TextEditOutcome, String> {
        self.inspector
            .point(frame, field, token, position, extend, &mut self.fields)
            .map_err(|error| error.to_string())
    }

    pub(super) fn selected_text(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<Option<String>, String> {
        self.inspector
            .selected_text(frame, field, token, &self.fields)
            .map_err(|error| error.to_string())
    }

    pub(super) fn is_composing(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<bool, String> {
        self.inspector
            .is_composing(frame, field, token)
            .map_err(|error| error.to_string())
    }

    pub(super) fn ime_area(
        &self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
    ) -> Result<UiRect, String> {
        self.inspector
            .ime_area(frame, field, token)
            .map_err(|error| error.to_string())
    }

    pub(super) fn input_error(
        &mut self,
        frame: &Arc<UiFrame>,
        field: &UiKey,
        token: UiTextInputToken,
        error: String,
    ) {
        self.inspector
            .input_error(frame, field, token, &mut self.fields, error);
    }

    pub(super) fn cancel_input(&mut self) -> Result<(), String> {
        self.inspector
            .cancel_input()
            .map_err(|error| error.to_string())
    }
}

fn bounded(value: &impl std::fmt::Display) -> String {
    value.to_string().chars().take(MAX_FEEDBACK_CHARS).collect()
}

#[cfg(all(test, any(windows, all(target_os = "linux", target_arch = "x86_64"))))]
mod tests;
