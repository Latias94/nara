use std::collections::BTreeMap;

use dear_imgui_rs::Ui;
use nara::{
    reflect::ComponentValue,
    scene::{SceneEntityId, ScenePatchOperation},
    tooling::{
        EditorActionOutcome, EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis,
        EditorFrameModel, EditorPropertyTarget, EditorSourceToolAction,
        EditorSourceToolActionOutcome, EditorSourceToolActivationId, EditorSourceToolActive,
        EditorSourceToolError, EditorSourceToolIntent, EditorSourceToolReview,
        EditorSourceToolSelectionOutcome, EditorSourceToolSelectionPhase,
    },
};

use super::{ImguiTextOwner, PanelActionDispatch, PanelSubmissionId, PanelSubmitError};

const MAX_NAME_BYTES: usize = 256;
const MAX_FEEDBACK_CHARS: usize = 1024;

#[derive(Clone, PartialEq, Eq)]
struct PaintBasis {
    activation: Option<EditorSourceToolActivationId>,
    document: Option<EditorDocumentQueryBasis>,
    selected: Option<SceneEntityId>,
}

struct IntegerDraft {
    activation: EditorSourceToolActivationId,
    target: EditorPropertyTarget,
    value: i64,
}

struct PanelDraft {
    activation: EditorSourceToolActivationId,
    basis: EditorDocumentQueryBasis,
    selected_intent: Option<EditorSourceToolIntent>,
    name: String,
    value: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubmissionKind {
    Integer,
    Panel,
    Review,
    Lifecycle,
}

/// Finite presentation state for one actual panel instance. The Session owns every candidate.
#[derive(Default)]
pub(super) struct SourceToolWidgets {
    basis: Option<PaintBasis>,
    integer: Option<IntegerDraft>,
    panel: Option<PanelDraft>,
    pending: Option<(PanelSubmissionId, SubmissionKind)>,
    offered: Option<SubmissionKind>,
    integer_painted: bool,
    serial: u64,
    text_owner: Option<ImguiTextOwner>,
    notice: Option<&'static str>,
    feedback: Option<String>,
}

impl SourceToolWidgets {
    pub(super) fn begin_paint(
        &mut self,
        frame: &EditorFrameModel,
        document: Option<EditorDocumentId>,
    ) {
        self.integer_painted = false;
        self.text_owner = None;
        self.offered = None;
        let basis = document.and_then(|document| document_basis(frame, document));
        let selected = basis
            .as_ref()
            .and_then(|basis| selected_entity(frame, basis))
            .cloned();
        let current = PaintBasis {
            activation: frame
                .source_tool()
                .active
                .as_ref()
                .map(|active| active.id().clone()),
            document: basis,
            selected,
        };
        if self.basis.as_ref() != Some(&current) {
            self.cancel_drafts(
                "Source-tool input cancelled because the activation, source, Schema or selection changed. Enter a new proposal against the current source.",
            );
            self.basis = Some(current);
        }
    }

    pub(super) fn end_paint(&mut self) {
        if !self.integer_painted && self.integer.take().is_some() {
            self.invalidate_controls();
            self.notice = Some(
                "Source-tool property input cancelled because its control is no longer visible. Reopen the property to start a new proposal.",
            );
            if self
                .pending
                .is_some_and(|(_, kind)| kind == SubmissionKind::Integer)
            {
                self.pending = None;
            }
        }
    }

    pub(super) fn text_owner(&self) -> Option<super::ImguiTextOwner> {
        self.text_owner
    }

    /// Returns false without painting when the ordinary field editor should retain the row.
    pub(super) fn show_integer_property(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        target: &EditorPropertyTarget,
        current: Option<i64>,
        editable: bool,
        review: Option<&EditorSourceToolReview<'_>>,
    ) -> (bool, Option<EditorSourceToolAction>) {
        let Some(active) = frame.source_tool().active.as_ref() else {
            return (false, None);
        };
        let Some(property) = active.integer_property() else {
            return (false, None);
        };
        let Some(current) = current.filter(|_| active.matches_integer_property(target)) else {
            return (false, None);
        };
        self.integer_painted = true;
        let target_current = self.basis.as_ref().is_some_and(|basis| {
            basis.activation.as_ref() == Some(active.id())
                && basis.document.as_ref() == Some(&target.basis)
                && basis.selected.as_ref() == Some(&target.entity)
        });
        if !target_current {
            self.cancel_drafts(
                "Source-tool property input cancelled: this property no longer matches the current document and selection.",
            );
            ui.text_disabled("Source-tool property is unavailable for this source observation");
            self.show_feedback(ui);
            return (true, None);
        }
        if self
            .integer
            .as_ref()
            .is_some_and(|draft| draft.activation != *active.id() || draft.target != *target)
        {
            self.cancel_drafts(
                "Source-tool property input cancelled because its original target changed. Enter a new parameter.",
            );
        }
        let draft = self.integer.get_or_insert_with(|| IntegerDraft {
            activation: active.id().clone(),
            target: target.clone(),
            value: property.initial(),
        });
        let _scope = ui.push_id(&format!("source-tool:integer:{}", self.serial));
        // Child metadata is rendered only as text, never as an ImGui label or ID.
        ui.text_wrapped(target.property.root.as_str());
        ui.text(format!("Current value: {current}"));
        ui.text_wrapped(property.label());
        let enabled = editable
            && self.pending.is_none()
            && self.offered.is_none()
            && !frame.source_tool().working;
        ui.with_disabled_if(!enabled, || {
            ui.input_scalar("##parameter", &mut draft.value).build();
        });
        record(ui, "source-tool:integer:parameter");
        #[cfg(test)]
        record_property(ui, target, "parameter");
        super::observe_text_owner(ui, &mut self.text_owner);
        let requested = button(
            ui,
            "Prepare property proposal",
            "source-tool:integer:prepare",
            enabled,
        );
        #[cfg(test)]
        record_property(ui, target, "Preview");
        let action = requested.then(|| EditorSourceToolAction::IntegerProperty {
            activation: draft.activation.clone(),
            target: draft.target.clone(),
            value: draft.value,
        });
        let mut action = action.and_then(|action| self.offer(action, SubmissionKind::Integer));
        self.show_feedback(ui);
        if let Some(review_action) = self.show_review(
            ui,
            frame,
            Some(&target.basis),
            Some(&target.entity),
            review,
            editable,
            true,
        ) {
            action = Some(review_action);
        }
        (true, action)
    }

    pub(super) fn show_panel(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        document: Option<EditorDocumentId>,
        review: Option<&EditorSourceToolReview<'_>>,
        editable: bool,
    ) -> Vec<EditorSourceToolAction> {
        let _scope = ui.push_id(&format!("source-tool:panel:{}", self.serial));
        let view = frame.source_tool();
        let basis = document.and_then(|document| document_basis(frame, document));
        let selected = basis
            .as_ref()
            .and_then(|basis| selected_entity(frame, basis));
        // Keep the shared candidate ahead of the form so acceptance is visible in a small panel.
        let mut action =
            self.show_review(ui, frame, basis.as_ref(), selected, review, editable, false);
        if view.probing {
            ui.text("Probing source-tool declaration...");
            if let Some(attempt) = &view.attempt
                && button(
                    ui,
                    "Cancel tool probe",
                    "source-tool:cancel-probe",
                    self.available(true),
                )
            {
                action = self.offer(
                    EditorSourceToolAction::CancelAttempt {
                        attempt: attempt.clone(),
                    },
                    SubmissionKind::Lifecycle,
                );
            }
        }
        if view.working {
            ui.text("Preparing source-tool proposal...");
        }
        if view.retiring {
            ui.text("Waiting for source-tool work to return...");
        }
        if let Some(error) = &view.attempt_error {
            ui.text_wrapped(format!("Tool probe: {}", bounded(error)));
        }
        if let Some(error) = &view.request_error {
            ui.text_wrapped(format!("Tool request: {}", bounded(error)));
        }
        if let Some(selection) = &view.selection {
            let phase = match selection.phase {
                EditorSourceToolSelectionPhase::Loading => "reading saved selection...",
                EditorSourceToolSelectionPhase::Idle => "idle",
                EditorSourceToolSelectionPhase::Probing => "probing approved selection...",
                EditorSourceToolSelectionPhase::Publishing => "publishing saved selection...",
                EditorSourceToolSelectionPhase::Reconciling => "reconciling saved selection...",
                EditorSourceToolSelectionPhase::Unavailable => "unavailable",
                EditorSourceToolSelectionPhase::RequiresReconcile => "requires reconciliation",
            };
            ui.text_wrapped(format!("Selection phase: {phase}"));
            for (label, attempt) in [
                ("Selection operation", selection.operation.as_ref()),
                ("Disable operation", selection.disable.as_ref()),
            ] {
                if let Some(attempt) = attempt {
                    let status = match &attempt.outcome {
                        None => "pending".to_owned(),
                        Some(EditorSourceToolSelectionOutcome::Observed) => "observed".to_owned(),
                        Some(EditorSourceToolSelectionOutcome::Enabled) => "enabled".to_owned(),
                        Some(EditorSourceToolSelectionOutcome::Disabled) => "disabled".to_owned(),
                        Some(EditorSourceToolSelectionOutcome::PublishedWithoutActivation) => {
                            "published without activation".to_owned()
                        }
                        Some(EditorSourceToolSelectionOutcome::Reconciled) => {
                            "reconciled".to_owned()
                        }
                        Some(EditorSourceToolSelectionOutcome::Failed(error)) => {
                            format!("failed: {error}")
                        }
                    };
                    ui.text_wrapped(format!("{label}: {status}"));
                }
            }
            if let Some(desired) = &selection.desired {
                ui.text_wrapped(format!(
                    "Installed selection: {} ({})",
                    desired.installed.package,
                    if desired.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
                if desired.enabled && selection.active_origin.as_ref() != Some(&desired.installed) {
                    ui.text_wrapped(
                        "The saved selection is awaiting explicit Host approval and activation.",
                    );
                }
                if view.active.is_none()
                    && button(
                        ui,
                        "Disable saved selection",
                        "source-tool:disable-saved",
                        self.available(true),
                    )
                {
                    action = self.offer(
                        EditorSourceToolAction::DisableInstalled {
                            basis: selection.basis.clone(),
                        },
                        SubmissionKind::Lifecycle,
                    );
                }
            }
            if let Some(error) = &selection.error {
                ui.text_wrapped(format!("Installed selection: {error}"));
            }
            if matches!(
                selection.phase,
                nara::tooling::EditorSourceToolSelectionPhase::Unavailable
                    | nara::tooling::EditorSourceToolSelectionPhase::RequiresReconcile
            ) && button(
                ui,
                "Read saved selection",
                "source-tool:reconcile",
                self.available(true),
            ) {
                action = self.offer(
                    EditorSourceToolAction::ReconcileSelection {
                        basis: selection.basis.clone(),
                    },
                    SubmissionKind::Lifecycle,
                );
            }
        }
        self.show_feedback(ui);
        let Some(active) = &view.active else {
            ui.text_disabled("No source tool is enabled by this Host");
            return action.into_iter().collect();
        };
        ui.text_wrapped(format!("Component: {}", active.component().as_str()));
        if button(
            ui,
            "Disable source tool",
            "source-tool:disable",
            self.available(true),
        ) {
            action = self.offer(
                match view
                    .selection
                    .as_ref()
                    .filter(|selection| selection.active_origin.is_some())
                {
                    Some(selection) => EditorSourceToolAction::DisableInstalled {
                        basis: selection.basis.clone(),
                    },
                    None => EditorSourceToolAction::Disable {
                        activation: active.id().clone(),
                    },
                },
                SubmissionKind::Lifecycle,
            );
        }
        let Some(basis) = basis else {
            ui.text_disabled("Choose an open Scene or Prefab source document");
            return action.into_iter().collect();
        };
        let selected = selected_entity(frame, &basis);
        let reason = panel_unavailable_reason(active, &basis);
        if let Some(reason) = reason {
            ui.text_wrapped(reason);
        }
        let enabled = self.available(editable) && !view.working && reason.is_none();
        let scope_current = self.basis.as_ref().is_some_and(|paint| {
            paint.activation.as_ref() == Some(active.id())
                && paint.document.as_ref() == Some(&basis)
                && paint.selected.as_ref() == selected
        });
        if !scope_current {
            ui.text_disabled(
                "Source-tool input cancelled: the panel observation changed before paint",
            );
            return action.into_iter().collect();
        }
        let draft = self.panel.get_or_insert_with(|| PanelDraft {
            activation: active.id().clone(),
            basis: basis.clone(),
            selected_intent: selected_intent(frame, &basis, active),
            name: "New entity".to_owned(),
            value: active
                .integer_property()
                .map_or(0, |property| property.initial()),
        });
        if let Some(property) = active.integer_property() {
            ui.text_wrapped(property.label());
            ui.with_disabled_if(!enabled, || {
                ui.input_scalar("##parameter", &mut draft.value).build();
            });
            record(ui, "source-tool:parameter");
            super::observe_text_owner(ui, &mut self.text_owner);
        }
        let selected_label = match &draft.selected_intent {
            Some(EditorSourceToolIntent::Edit { .. }) => "Prepare selected edit",
            _ => "Prepare attachment",
        };
        if let Some(entity) = selected {
            ui.text_wrapped(format!("Selected entity: {}", entity.as_str()));
        } else {
            ui.text_disabled("Select a directly projected source entity to Attach or Edit");
        }
        let selected_clicked = button(
            ui,
            selected_label,
            "source-tool:attach-or-edit",
            enabled && draft.selected_intent.is_some(),
        );
        #[cfg(test)]
        super::interaction_tests::record_control(
            ui,
            match &draft.selected_intent {
                Some(EditorSourceToolIntent::Edit { .. }) => "source-tool-panel:Edit",
                _ => "source-tool-panel:Attach",
            },
        );
        ui.separator();
        ui.text("Spawn name");
        ui.with_disabled_if(!enabled, || {
            ui.input_text("##spawn-name", &mut draft.name).build();
        });
        record(ui, "source-tool:spawn-name");
        super::observe_text_owner(ui, &mut self.text_owner);
        let oversized = draft.name.len() > MAX_NAME_BYTES;
        if oversized {
            truncate_utf8(&mut draft.name, MAX_NAME_BYTES);
            self.notice = Some(
                "Spawn name was limited to 256 UTF-8 bytes. Review the name before preparing a proposal.",
            );
        }
        let valid_name = !draft.name.trim().is_empty() && !draft.name.chars().any(char::is_control);
        if !valid_name {
            ui.text_disabled("Enter a nonempty Spawn name without control characters");
        }
        let spawn_clicked = button(
            ui,
            "Prepare Spawn",
            "source-tool:spawn",
            enabled && valid_name && !oversized && !selected_clicked,
        );
        let intent = if selected_clicked {
            draft.selected_intent.clone()
        } else if spawn_clicked {
            Some(EditorSourceToolIntent::Spawn {
                name: draft.name.clone(),
            })
        } else {
            None
        };
        let request = intent.map(|intent| EditorSourceToolAction::Request {
            activation: draft.activation.clone(),
            basis: draft.basis.clone(),
            intent,
            parameters: active
                .integer_property()
                .map_or_else(BTreeMap::new, |property| {
                    BTreeMap::from([(
                        property.parameter().to_owned(),
                        ComponentValue::I64(draft.value),
                    )])
                }),
        });
        if let Some(request) = request {
            action = self.offer(request, SubmissionKind::Panel);
        }
        if oversized {
            self.invalidate_controls();
        }
        action.into_iter().collect()
    }

    pub(super) fn admitted(&mut self, result: Result<PanelSubmissionId, PanelSubmitError>) {
        let Some(kind) = self.offered else {
            return;
        };
        match result {
            Ok(id) if self.pending.is_none() => {
                self.pending = Some((id, kind));
                self.feedback = None;
            }
            Ok(_) => {}
            Err(error) => {
                self.feedback = Some(format!(
                    "Source-tool action was not queued: {}. Review and try again.",
                    bounded(&error)
                ));
            }
        }
    }

    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        if self
            .pending
            .as_ref()
            .is_none_or(|(pending, _)| *pending != id)
        {
            return;
        }
        self.pending = None;
        let outcome = match result {
            PanelActionDispatch::Dispatched(EditorActionOutcome::SourceTool(outcome)) => outcome,
            PanelActionDispatch::NotDispatched(reason) => {
                self.feedback = Some(format!(
                    "Source-tool action was not dispatched: {reason:?}. Review the current source before trying again."
                ));
                return;
            }
            PanelActionDispatch::Dispatched(_) => {
                self.feedback = Some("Source-tool action was rejected by the Editor. Review the current source and Diagnostics before trying again.".to_owned());
                return;
            }
        };
        let error = match outcome {
            EditorSourceToolActionOutcome::Selection(result) => {
                self.feedback = Some(match result {
                    Ok(_) => "Installed selection requested. Observe its durable result separately from physical retirement.".to_owned(),
                    Err(error) => format!("Installed selection: {error}"),
                });
                return;
            }
            EditorSourceToolActionOutcome::Requested(Ok(_)) => {
                self.feedback =
                    Some("Proposal requested. Waiting for the Session's review.".to_owned());
                return;
            }
            EditorSourceToolActionOutcome::Accepted(Ok(commit)) => {
                self.integer = None;
                self.panel = None;
                self.invalidate_controls();
                self.feedback = Some(if commit.report.applied {
                    "Reviewed source-tool proposal committed. Use ordinary Undo to revert it."
                        .to_owned()
                } else {
                    "The source owner did not apply the reviewed proposal. Check Diagnostics."
                        .to_owned()
                });
                return;
            }
            EditorSourceToolActionOutcome::Disabled(Ok(())) => {
                self.cancel_drafts(
                    "Source-tool input cancelled because this activation was disabled.",
                );
                self.feedback = Some("Source tool disabled.".to_owned());
                return;
            }
            EditorSourceToolActionOutcome::Cancelled(Ok(())) => {
                self.feedback = Some(
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
            self.cancel_drafts("Source-tool input cancelled because its original activation or source observation is no longer current. Start a new proposal explicitly.");
        }
        self.feedback = Some(format!("Source-tool action rejected: {}", bounded(error)));
    }

    fn available(&self, editable: bool) -> bool {
        editable && self.pending.is_none() && self.offered.is_none()
    }

    fn offer(
        &mut self,
        action: EditorSourceToolAction,
        kind: SubmissionKind,
    ) -> Option<EditorSourceToolAction> {
        if !self.available(true) {
            return None;
        }
        self.offered = Some(kind);
        Some(action)
    }

    fn invalidate_controls(&mut self) {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("source-tool control identity exhausted");
        self.text_owner = None;
    }

    fn cancel_drafts(&mut self, notice: &'static str) {
        let had_input = self.integer.take().is_some() | self.panel.take().is_some();
        self.pending = None;
        self.invalidate_controls();
        if had_input {
            self.notice = Some(notice);
        }
    }

    pub(super) fn show_feedback(&self, ui: &Ui) {
        if let Some(notice) = self.notice {
            ui.text_wrapped(notice);
        }
        if let Some(feedback) = &self.feedback {
            ui.text_wrapped(feedback);
        }
        if self.pending.is_some() {
            ui.text_disabled("Source-tool action queued");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_review(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        basis: Option<&EditorDocumentQueryBasis>,
        selected: Option<&SceneEntityId>,
        review: Option<&EditorSourceToolReview<'_>>,
        editable: bool,
        integer: bool,
    ) -> Option<EditorSourceToolAction> {
        let view = frame.source_tool();
        let summary = view.review.as_ref()?;
        ui.separator();
        ui.text("Source-tool review");
        let document = frame
            .workspace()
            .document(summary.basis.document)
            .map_or("Closed document", |document| document.title.as_str());
        ui.text_wrapped(format!("Document: {document}"));
        match &summary.intent {
            EditorSourceToolIntent::Attach { entity } => {
                ui.text_wrapped(format!("Attach to {}", entity.as_str()))
            }
            EditorSourceToolIntent::Edit { entity } => {
                ui.text_wrapped(format!("Edit {}", entity.as_str()))
            }
            EditorSourceToolIntent::Spawn { name } => ui.text_wrapped(format!("Create {name}")),
        }
        let Some(review) = review.filter(|review| {
            review.request() == &summary.request && view.request.as_ref() == Some(review.request())
        }) else {
            ui.text_disabled("The exact prepared candidate is not available in this paint");
            return None;
        };
        // Borrow the exact candidate. Never derive replacement writes from the parameter draft.
        ui.text_wrapped(review.title());
        for operation in &review.patch().operations {
            match operation {
                ScenePatchOperation::SetField {
                    entity,
                    component,
                    field,
                    value,
                    ..
                } => {
                    ui.text_wrapped(format!(
                        "{} / {} / {}: {}",
                        entity.as_str(),
                        component.as_str(),
                        field.as_str(),
                        super::authoring::value_label(Some(value))
                    ));
                }
                ScenePatchOperation::AddEntity { entity, .. } => {
                    ui.text_wrapped(format!("Create entity {}", entity.id.as_str()))
                }
                ScenePatchOperation::AddComponent {
                    entity, component, ..
                } => ui.text_wrapped(format!("Add {} to {}", component.as_str(), entity.as_str())),
                other => ui.text_wrapped(format!("{other:?}")),
            }
        }
        let source_matches = basis == Some(&summary.basis);
        let intent_matches = match &summary.intent {
            EditorSourceToolIntent::Attach { entity } | EditorSourceToolIntent::Edit { entity } => {
                selected == Some(entity)
            }
            EditorSourceToolIntent::Spawn { .. } => !integer,
        };
        if !source_matches {
            ui.text_disabled("Review belongs to an older or different document observation; prepare a new proposal");
        } else if !intent_matches {
            ui.text_disabled("Review belongs to another entity or intent; select its original target to accept it");
        }
        let enabled = self.available(editable)
            && source_matches
            && intent_matches
            && view.active.is_some()
            && !view.working;
        let identity = if integer {
            "source-tool:integer:accept"
        } else {
            "source-tool:accept"
        };
        let accept = button(ui, "Accept reviewed proposal", identity, enabled);
        #[cfg(test)]
        if integer {
            if let Some(draft) = &self.integer {
                record_property(ui, &draft.target, "Accept");
            }
        } else {
            super::interaction_tests::record_control(ui, "source-tool-panel:Accept");
        }
        accept
            .then(|| {
                self.offer(
                    EditorSourceToolAction::Accept {
                        request: review.request().clone(),
                    },
                    SubmissionKind::Review,
                )
            })
            .flatten()
    }
}

fn document_basis(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
) -> Option<EditorDocumentQueryBasis> {
    let tab = frame.workspace().document(document)?;
    matches!(
        tab.kind,
        EditorDocumentKind::Scene | EditorDocumentKind::Prefab
    )
    .then(|| EditorDocumentQueryBasis {
        document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    })
}

fn selected_entity<'a>(
    frame: &'a EditorFrameModel,
    basis: &EditorDocumentQueryBasis,
) -> Option<&'a SceneEntityId> {
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .filter(|scene| scene.document == basis.document && scene.revision == basis.revision)?;
    scene.editor.inspector.selected_entity.as_ref()
}

fn selected_intent(
    frame: &EditorFrameModel,
    basis: &EditorDocumentQueryBasis,
    active: &EditorSourceToolActive,
) -> Option<EditorSourceToolIntent> {
    let selected = selected_entity(frame, basis)?;
    let entity = frame
        .workspace()
        .active_scene
        .as_ref()?
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .filter(|entity| &entity.id == selected)?;
    Some(
        if entity
            .components
            .iter()
            .any(|component| &component.component == active.component())
        {
            EditorSourceToolIntent::Edit {
                entity: selected.clone(),
            }
        } else {
            EditorSourceToolIntent::Attach {
                entity: selected.clone(),
            }
        },
    )
}

fn panel_unavailable_reason(
    active: &EditorSourceToolActive,
    basis: &EditorDocumentQueryBasis,
) -> Option<&'static str> {
    if basis.schema.is_none() {
        return Some("Source-tool proposals require an admitted document Schema");
    }
    if !active.matches_schema(basis) {
        return Some("The document selects another component owner or declaration");
    }
    None
}

fn button(ui: &Ui, label: &'static str, identity: &'static str, enabled: bool) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    record(ui, identity);
    clicked
}

fn record(_ui: &Ui, _identity: &'static str) {
    #[cfg(test)]
    super::interaction_tests::record_control(_ui, _identity);
}

#[cfg(test)]
fn record_property(ui: &Ui, target: &EditorPropertyTarget, control: &str) {
    super::interaction_tests::record_control(
        ui,
        format!(
            "source-tool-property:{}:{}:{control}",
            target.component.as_str(),
            target.property.root.as_str(),
        ),
    );
}

fn bounded(value: &impl std::fmt::Display) -> String {
    value.to_string().chars().take(MAX_FEEDBACK_CHARS).collect()
}

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    let mut boundary = value.len().min(max_bytes);
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
}

#[cfg(test)]
mod tests;
