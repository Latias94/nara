//! Presentation and ordered commands for the Host's retained conversion operation.

use dear_imgui_rs::Ui;
use nara::{
    diagnostic::DiagnosticReport,
    scene::PrefabInstancePath,
    tooling::{
        EditorActionOutcome, EditorDocumentId, EditorDocumentQueryBasis,
        EditorPrefabConversionAction, EditorPrefabConversionActionOutcome,
        EditorPrefabConversionId, EditorPrefabConversionPhase, EditorPrefabConversionResult,
        EditorPrefabConversionView,
    },
};

use super::{ImguiEditorPanelContext, PanelActionDispatch, PanelSubmissionId, button};

const CONVERT_LABEL: &str = "Convert Entire Instance to Local";
const CONVERSION_SCOPE: &str = "Converts the entire selected instance, including all nested projected instances and overlay additions. The anchor and its ordinary local children are preserved; other instances stay linked.";

#[derive(Default)]
pub(super) struct ConversionUi {
    operation: Option<EditorPrefabConversionId>,
    owner: Option<ConversionOwner>,
    phase: EditorPrefabConversionPhase,
    observed: bool,
    pending: Option<ConversionSubmission>,
    acknowledged: bool,
    notice: Option<String>,
    mapping_focus: Option<nara::scene::SceneEntityId>,
}

struct ConversionOwner {
    document: EditorDocumentId,
    title: String,
    anchor: String,
}

#[derive(Clone, Copy)]
struct ConversionSubmission {
    id: PanelSubmissionId,
    operation: Option<EditorPrefabConversionId>,
    command: ConversionCommand,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConversionCommand {
    Prepare,
    Commit,
    Cancel,
    ReviewPage,
    Acknowledge,
}

impl ConversionUi {
    pub(super) fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) -> bool {
        let frame = context.input.frame;
        let view = frame.prefab_conversion();
        let title = view.document.and_then(|document| {
            frame
                .workspace()
                .document(document)
                .map(|tab| tab.title.as_str())
        });
        let editing = !context.input.read_only && super::super::authoring::editing_enabled(frame);
        let (visible, action) = self.paint(ui, view, title, editing);
        if let Some(action) = action {
            self.submit(context, action);
        }
        visible
    }

    pub(super) fn show_prepare(
        &mut self,
        ui: &Ui,
        context: &mut ImguiEditorPanelContext<'_, '_>,
        basis: Option<&EditorDocumentQueryBasis>,
        path: Option<&PrefabInstancePath>,
        has_instance: bool,
        enabled: bool,
    ) {
        if let Some(EditorPrefabConversionAction::Prepare { basis, anchor }) =
            paint_prepare(ui, basis, path, has_instance, enabled)
        {
            let title = context
                .input
                .frame
                .workspace()
                .document(basis.document)
                .map_or("Source document", |tab| tab.title.as_str());
            self.owner = Some(ConversionOwner {
                document: basis.document,
                title: title.to_owned(),
                anchor: anchor.as_str().to_owned(),
            });
            self.submit(
                context,
                EditorPrefabConversionAction::Prepare { basis, anchor },
            );
        }
    }

    fn submit(
        &mut self,
        context: &mut ImguiEditorPanelContext<'_, '_>,
        action: EditorPrefabConversionAction,
    ) {
        if self.pending.is_some() {
            return;
        }
        let (command, operation) = match &action {
            EditorPrefabConversionAction::Prepare { .. } => (ConversionCommand::Prepare, None),
            EditorPrefabConversionAction::Commit { operation } => {
                (ConversionCommand::Commit, Some(*operation))
            }
            EditorPrefabConversionAction::Cancel { operation } => {
                (ConversionCommand::Cancel, Some(*operation))
            }
            EditorPrefabConversionAction::ReviewPage { operation, .. } => {
                (ConversionCommand::ReviewPage, Some(*operation))
            }
            EditorPrefabConversionAction::Acknowledge { operation } => {
                (ConversionCommand::Acknowledge, Some(*operation))
            }
        };
        match context.submit_prefab_conversion(action) {
            Ok(id) => {
                self.pending = Some(ConversionSubmission {
                    id,
                    operation,
                    command,
                });
                self.notice = None;
            }
            Err(error) => {
                self.notice = Some(format!("Conversion command was not queued: {error}."))
            }
        }
    }

    pub(super) fn action_finished(
        &mut self,
        id: PanelSubmissionId,
        result: &PanelActionDispatch,
    ) -> bool {
        let Some(pending) = self.pending.filter(|pending| pending.id == id) else {
            return false;
        };
        self.pending = None;
        match result {
            PanelActionDispatch::Dispatched(EditorActionOutcome::PrefabConversion(
                EditorPrefabConversionActionOutcome::Accepted { operation, phase },
            )) if pending
                .operation
                .is_none_or(|expected| expected == *operation) =>
            {
                if self.operation != Some(*operation) {
                    self.observed = false;
                    self.mapping_focus = None;
                }
                self.operation = Some(*operation);
                self.phase = *phase;
                self.acknowledged = pending.command == ConversionCommand::Acknowledge;
                self.notice = None;
            }
            PanelActionDispatch::Dispatched(EditorActionOutcome::PrefabConversion(
                EditorPrefabConversionActionOutcome::Rejected { reason, .. },
            )) => {
                self.notice = Some(format!("Conversion command rejected: {reason:?}."));
            }
            PanelActionDispatch::NotDispatched(reason) => {
                self.notice = Some(format!(
                    "Conversion command was not dispatched: {reason:?}."
                ));
            }
            _ => {
                self.notice = Some("Conversion dispatch returned an unrelated outcome. No conversion completion has been established.".into());
            }
        }
        true
    }

    fn observe(&mut self, view: &EditorPrefabConversionView, title: Option<&str>) {
        if let Some(operation) = view.operation {
            if self.operation != Some(operation) {
                self.operation = Some(operation);
                self.owner = None;
                self.mapping_focus = None;
                self.acknowledged = false;
                self.phase = view.phase;
            } else if !matches!(
                (self.phase, view.phase),
                (
                    EditorPrefabConversionPhase::Validating
                        | EditorPrefabConversionPhase::Cancelling,
                    EditorPrefabConversionPhase::Preparing | EditorPrefabConversionPhase::Review
                )
            ) {
                self.phase = view.phase;
            }
            self.observed = true;
            if self.owner.is_none()
                && let Some((document, anchor)) = view.document.zip(view.anchor.as_ref())
            {
                self.owner = Some(ConversionOwner {
                    document,
                    title: title.unwrap_or("Source document").to_owned(),
                    anchor: anchor.to_string(),
                });
            }
        } else if self.acknowledged || self.observed {
            self.operation = None;
            self.owner = None;
            self.mapping_focus = None;
            self.phase = EditorPrefabConversionPhase::Idle;
            self.observed = false;
            self.acknowledged = false;
        }
    }

    /// Returns commands only for explicit controls; observing another frame never starts work.
    fn paint(
        &mut self,
        ui: &Ui,
        view: &EditorPrefabConversionView,
        title: Option<&str>,
        editing: bool,
    ) -> (bool, Option<EditorPrefabConversionAction>) {
        self.observe(view, title);
        if self.operation.is_none() && self.pending.is_none() {
            if let Some(notice) = &self.notice {
                ui.text_wrapped(notice);
            }
            return (false, None);
        }
        ui.text(CONVERT_LABEL);
        ui.text_wrapped(CONVERSION_SCOPE);
        if let Some(owner) = &self.owner {
            ui.text_wrapped(format!(
                "Original source: {} (document {}); anchor: {}",
                owner.title,
                owner.document.raw(),
                owner.anchor,
            ));
            #[cfg(test)]
            super::super::ui_interaction_tests::record_control(
                ui,
                format!(
                    "Prefab Intent conversion owner: {} / {}",
                    owner.document.raw(),
                    owner.anchor
                ),
            );
        }
        ui.text_disabled("Navigation does not retarget or prepare this operation.");
        if let Some(notice) = &self.notice {
            ui.text_wrapped(notice);
        }
        if let Some(pending) = &self.pending {
            ui.text_wrapped(match pending.command {
                ConversionCommand::Prepare => "Preparation queued; waiting for dispatch acceptance. No source changes have been committed.",
                ConversionCommand::Commit => "Commit queued; waiting for dispatch acceptance and final validation. Conversion is not yet complete.",
                ConversionCommand::Cancel => "Cancellation queued; waiting for the Host's terminal result.",
                ConversionCommand::ReviewPage => "Mapping page requested; the same prepared conversion remains in Host custody.",
                ConversionCommand::Acknowledge => "Result acknowledgement queued.",
            });
        }
        let Some(operation) = self.operation else {
            return (true, None);
        };
        let current = view.operation == Some(operation);
        let result = current
            .then_some(view.result.as_ref())
            .flatten()
            .filter(|result| result_operation(result) == operation);
        if let Some(result) = result {
            show_result(ui, result);
            let acknowledge = button(
                ui,
                "Acknowledge conversion result",
                self.pending.is_none() && !self.acknowledged,
            );
            return (
                true,
                acknowledge.then_some(EditorPrefabConversionAction::Acknowledge { operation }),
            );
        }
        ui.text_wrapped(phase_text(self.phase));
        #[cfg(test)]
        super::super::ui_interaction_tests::record_control(
            ui,
            format!("Prefab Intent conversion phase: {:?}", self.phase),
        );
        let review = current
            .then_some(view.review.as_deref())
            .flatten()
            .filter(|_| self.phase == EditorPrefabConversionPhase::Review);
        let mut action = None;
        if let Some(review) = review {
            ui.text_wrapped(format!(
                "{} new local entities; {} actual Prefab sources; {} working copies.",
                review.total_entities, review.source_count, review.working_copy_count,
            ));
            ui.text_wrapped(format!(
                "Mapping rows {}..{} of {} (projected origin -> new local ID)",
                review.offset,
                review.offset.saturating_add(review.rows.len()),
                review.total_entities,
            ));
            ui.child_window("prefab-conversion-mapping")
                .size([
                    0.0,
                    (ui.content_region_avail()[1] - 170.0).clamp(48.0, 180.0),
                ])
                .build(ui, || {
                    for row in &review.rows {
                        if ui
                            .selectable_config(format!("Local {}", row.entity))
                            .selected(self.mapping_focus.as_ref() == Some(&row.entity))
                            .build()
                        {
                            self.mapping_focus = Some(row.entity.clone());
                        }
                        if ui.is_item_hovered() {
                            ui.tooltip_text(&row.origin);
                        }
                        #[cfg(test)]
                        {
                            super::super::ui_interaction_tests::record_control(
                                ui,
                                format!("Prefab Intent: Local {}", row.entity),
                            );
                            super::super::ui_interaction_tests::record_control(
                                ui,
                                format!(
                                    "Prefab Intent conversion mapping: {} -> {}",
                                    row.origin, row.entity
                                ),
                            );
                        }
                    }
                });
            if let Some(row) = review
                .rows
                .iter()
                .find(|row| self.mapping_focus.as_ref() == Some(&row.entity))
            {
                ui.child_window("prefab-conversion-mapping-detail")
                    .size([0.0, 80.0])
                    .build(ui, || {
                        ui.text_wrapped(format!("Source: {}", row.origin));
                        #[cfg(test)]
                        super::super::ui_interaction_tests::record_control(
                            ui,
                            "Prefab Intent conversion source detail",
                        );
                        ui.text_wrapped(format!("New local ID: {}", row.entity));
                    });
            } else {
                ui.text_disabled("Select a local ID to inspect its complete source path.");
            }
            let available = self.pending.is_none();
            if button(
                ui,
                "First conversion mapping page",
                available && review.offset != 0,
            ) {
                action = Some(EditorPrefabConversionAction::ReviewPage {
                    operation,
                    offset: 0,
                });
            }
            if button(
                ui,
                "Next conversion mapping page",
                available && review.next_offset.is_some(),
            ) && let Some(offset) = review.next_offset
            {
                action = Some(EditorPrefabConversionAction::ReviewPage { operation, offset });
            }
            ui.text_wrapped("Commit validates the original source and actual dependencies again, then publishes one Undo step. A rejected or cancelled operation publishes no conversion.");
            if button(ui, "Commit reviewed conversion", editing && available) {
                action = Some(EditorPrefabConversionAction::Commit { operation });
            }
        }
        let cancel = self.pending.is_none()
            && matches!(
                self.phase,
                EditorPrefabConversionPhase::Preparing
                    | EditorPrefabConversionPhase::Review
                    | EditorPrefabConversionPhase::Validating
            );
        if button(ui, "Cancel conversion", cancel) {
            action = Some(EditorPrefabConversionAction::Cancel { operation });
        }
        (true, action)
    }
}

fn paint_prepare(
    ui: &Ui,
    basis: Option<&EditorDocumentQueryBasis>,
    path: Option<&PrefabInstancePath>,
    has_instance: bool,
    enabled: bool,
) -> Option<EditorPrefabConversionAction> {
    let eligible = basis.is_some()
        && path.is_some_and(|path| path.steps.is_empty())
        && has_instance
        && enabled;
    let clicked = button(ui, CONVERT_LABEL, eligible);
    ui.text_wrapped(CONVERSION_SCOPE);
    #[cfg(test)]
    super::super::ui_interaction_tests::record_control(ui, "Prefab Intent conversion scope");
    if path.is_some_and(|path| !path.steps.is_empty()) {
        ui.text_disabled("Conversion requires a direct source anchor. A nested stored declaration is unavailable for this action.");
    }
    prepare_action(basis, path, has_instance, enabled && clicked)
}

fn prepare_action(
    basis: Option<&EditorDocumentQueryBasis>,
    path: Option<&PrefabInstancePath>,
    has_instance: bool,
    enabled: bool,
) -> Option<EditorPrefabConversionAction> {
    let (basis, path) = basis.zip(path)?;
    (enabled && has_instance && path.steps.is_empty()).then(|| {
        EditorPrefabConversionAction::Prepare {
            basis: basis.clone(),
            anchor: path.anchor.clone(),
        }
    })
}

fn result_operation(result: &EditorPrefabConversionResult) -> EditorPrefabConversionId {
    match result {
        EditorPrefabConversionResult::Committed { operation, .. }
        | EditorPrefabConversionResult::Cancelled { operation }
        | EditorPrefabConversionResult::Failed { operation, .. } => *operation,
    }
}

fn phase_text(phase: EditorPrefabConversionPhase) -> &'static str {
    match phase {
        EditorPrefabConversionPhase::Idle => "Waiting for the Host's operation observation.",
        EditorPrefabConversionPhase::Preparing => {
            "Preparing the complete instance in the background. No source changes have been committed."
        }
        EditorPrefabConversionPhase::Review => {
            "Review the retained conversion and its fixed local IDs before committing."
        }
        EditorPrefabConversionPhase::Validating => {
            "Commit accepted; validating source files and working copies. Conversion is not yet complete."
        }
        EditorPrefabConversionPhase::Cancelling => {
            "Cancelling; waiting for the Host's terminal result."
        }
        EditorPrefabConversionPhase::Terminal => {
            "Waiting for the exact terminal conversion result."
        }
    }
}

fn show_result(ui: &Ui, result: &EditorPrefabConversionResult) {
    match result {
        EditorPrefabConversionResult::Committed { receipt, .. } => {
            ui.text_wrapped(format!(
                "Conversion committed: {} entities are now local. Undo restores the original linked instance.",
                receipt.entities().len(),
            ));
            #[cfg(test)]
            super::super::ui_interaction_tests::record_control(
                ui,
                "Prefab Intent conversion result: Committed",
            );
            show_diagnostics(ui, &receipt.report().diagnostics);
        }
        EditorPrefabConversionResult::Cancelled { .. } => {
            ui.text_wrapped("Conversion cancelled. No conversion was committed.");
            #[cfg(test)]
            super::super::ui_interaction_tests::record_control(
                ui,
                "Prefab Intent conversion result: Cancelled",
            );
        }
        EditorPrefabConversionResult::Failed {
            reason,
            diagnostics,
            ..
        } => {
            ui.text_wrapped(format!(
                "Conversion failed: {reason:?}. No conversion was committed."
            ));
            #[cfg(test)]
            super::super::ui_interaction_tests::record_control(
                ui,
                "Prefab Intent conversion result: Failed",
            );
            show_diagnostics(ui, diagnostics);
        }
    }
}

fn show_diagnostics(ui: &Ui, diagnostics: &DiagnosticReport) {
    if diagnostics.is_empty() {
        return;
    }
    ui.text_wrapped(format!(
        "{} observed diagnostics; {} retained.",
        diagnostics.len(),
        diagnostics.retained_len(),
    ));
    ui.child_window("prefab-conversion-diagnostics")
        .size([0.0, 140.0])
        .build(ui, || {
            for diagnostic in diagnostics {
                ui.text_wrapped(format!(
                    "{} [{:?}]: {}",
                    diagnostic.code().as_str(),
                    diagnostic.severity(),
                    diagnostic.summary().as_str(),
                ));
                #[cfg(test)]
                super::super::ui_interaction_tests::record_control(
                    ui,
                    format!(
                        "Prefab Intent conversion diagnostic: {}",
                        diagnostic.code().as_str()
                    ),
                );
                for field in diagnostic.fields() {
                    ui.text_wrapped(format!("{field:?}"));
                }
            }
        });
}

#[cfg(test)]
mod tests;
