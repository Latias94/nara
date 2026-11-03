use std::collections::BTreeSet;

use dear_imgui_rs::{Condition, ListClipper, Ui};
use nara::{
    asset::{AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    project_host::{
        EditorPrefabExtractionError, EditorPrefabExtractionId, EditorPrefabExtractionPhase,
        EditorPrefabExtractionRequest,
    },
    scene::SceneEntityId,
    tooling::{
        EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis, EditorFrameModel,
        EditorFrameRevision,
    },
};

use super::{ImguiTextOwner, observe_text_owner};
use crate::EditorApplication;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct ExtractionUi {
    draft: Option<ExtractionDraft>,
    save_current: BTreeSet<EditorDocumentId>,
    selection_operation: Option<EditorPrefabExtractionId>,
    dismissed: Option<EditorPrefabExtractionId>,
    notice: Option<String>,
    preview_path: Option<String>,
    preview_offset: usize,
}

struct ExtractionDraft {
    basis: EditorDocumentQueryBasis,
    root: SceneEntityId,
    title: String,
    alias: String,
}

pub(crate) struct ExtractionAction {
    generation: Option<u64>,
    command: ExtractionCommand,
}

enum ExtractionCommand {
    Prepare {
        request: EditorPrefabExtractionRequest,
        basis: EditorDocumentQueryBasis,
    },
    Revise(EditorPrefabExtractionId, Vec<EditorDocumentId>),
    Confirm(EditorPrefabExtractionId, EditorFrameRevision),
    Cancel(EditorPrefabExtractionId),
    Retry(EditorPrefabExtractionId),
}

impl ExtractionUi {
    pub(super) fn show_start(&mut self, ui: &Ui, frame: &EditorFrameModel, enabled: bool) {
        let selected = frame.workspace().active_scene.as_ref().and_then(|scene| {
            let document = frame.workspace().document(scene.document)?;
            (scene.kind == EditorDocumentKind::Prefab).then_some(())?;
            let root = scene.editor.inspector.selected_entity.as_ref()?;
            Some((document, root))
        });
        if button(
            ui,
            "Extract Prefab Branch...",
            enabled && selected.is_some(),
        ) {
            if let Some((document, root)) = selected {
                self.draft = Some(ExtractionDraft {
                    basis: EditorDocumentQueryBasis {
                        document: document.document,
                        revision: document.revision,
                        schema: document.authoring_schema_basis.clone(),
                    },
                    root: root.clone(),
                    title: document.title.clone(),
                    alias: String::new(),
                });
                self.save_current.clear();
                self.selection_operation = None;
                self.notice = None;
            }
        }
    }

    pub(super) fn show(
        &mut self,
        ui: &Ui,
        application: &EditorApplication,
        text_owner: &mut Option<ImguiTextOwner>,
    ) -> Option<ExtractionAction> {
        let view = application.prefab_extraction_view().ok()?;
        let frame_revision = application.current_frame()?.revision();
        let operation = view
            .operation
            .filter(|operation| self.draft.is_none() && self.dismissed != Some(*operation));
        if self.draft.is_none() && operation.is_none() {
            return None;
        }
        let accepted_selection = operation
            .and_then(|operation| application.prefab_extraction_review(operation).ok())
            .map(|review| {
                review
                    .documents()
                    .filter_map(|(document, disposition)| {
                        (disposition
                            == nara::tooling::EditorPrefabExtractionDisposition::SaveCurrent)
                            .then_some(document)
                    })
                    .collect::<BTreeSet<_>>()
            });
        if self.selection_operation != operation {
            if let Some(accepted) = &accepted_selection {
                self.save_current = accepted.clone();
                self.selection_operation = operation;
            }
        }
        let mut command = None;
        ui.window("Prefab extraction")
            .size([560.0, 640.0], Condition::FirstUseEver)
            .size_constraints([360.0, 280.0], [900.0, 800.0])
            .build(|| {
                if let Some(notice) = &self.notice {
                    ui.text_wrapped(notice);
                }
                if let Some(draft) = &mut self.draft {
                    ui.text_wrapped(format!("Source: {} / {}", draft.title, draft.root));
                    ui.text_wrapped("Extract this source-owned branch into a new Prefab. Preparation is read-only. The complete affected project is reviewed before publication.");
                    ui.input_text("Prefab alias", &mut draft.alias).build();
                    #[cfg(test)]
                    super::interaction_tests::record_control(ui, "Extraction Prefab alias");
                    observe_text_owner(ui, text_owner);
                    ui.text_disabled("Example: actors/player.prefab.json");
                    if button(ui, "Prepare extraction review", !draft.alias.trim().is_empty()) {
                        let target = AssetPath::new(draft.alias.trim())
                            .map_err(|error| error.to_string())
                            .and_then(|alias| {
                                StableAssetId::try_random()
                                    .map(|stable| AssetMetaIdentity::new(stable, alias, AssetSourceKind::Prefab))
                                    .map_err(|error| error.to_string())
                            });
                        match target {
                            Ok(target) => command = Some(ExtractionCommand::Prepare {
                                request: EditorPrefabExtractionRequest::new(draft.basis.document, draft.root.clone(), target),
                                basis: draft.basis.clone(),
                            }),
                            Err(error) => self.notice = Some(error),
                        }
                    }
                    if button(ui, "Discard extraction draft", true) {
                        self.draft = None;
                        self.notice = None;
                    }
                    return;
                }
                let Some(operation) = operation else { return };
                ui.text_wrapped(format!("Operation: {operation:?}; phase: {:?}", view.phase));
                ui.text_wrapped(format!("{} captured sources; {} open documents", view.sources, view.documents));
                if let Some(outcome) = view.outcome {
                    ui.text_wrapped(format!("Result: {outcome:?}"));
                }
                if let Some(refusal) = view.refusal {
                    ui.text_wrapped(format!("Refused: {refusal}"));
                }
                if let Some(diagnostics) = application.prefab_extraction_diagnostics() {
                    ui.child_window("extraction-diagnostics").size([0.0, 100.0]).build(ui, || {
                        for diagnostic in diagnostics {
                            ui.text_wrapped(format!("{}: {}", diagnostic.code().as_str(), diagnostic.summary().as_str()));
                        }
                    });
                }
                if let Ok(selection) = application.prefab_extraction_selection(operation) {
                        ui.child_window("extraction-save-selection").size([0.0, 100.0]).build(ui, || {
                            let mut clipper = ListClipper::new(selection.documents().len()).begin(ui);
                            while clipper.step() {
                                for source in selection.documents().skip(clipper.display_start()).take(clipper.display_end() - clipper.display_start()) {
                                    let _id = ui.push_id(source.document().raw().to_string().as_str());
                                    let mut selected = self.save_current.contains(&source.document());
                                    if ui.checkbox("Save current", &mut selected) {
                                        if selected { self.save_current.insert(source.document()); }
                                        else { self.save_current.remove(&source.document()); }
                                    }
                                    #[cfg(test)]
                                    super::interaction_tests::record_control(ui, format!("Extraction save:{}", source.project_path()));
                                    ui.same_line();
                                    ui.text(source.project_path());
                                    if ui.is_item_hovered() { ui.tooltip_text(source.project_path()); }
                                }
                            }
                        });
                        if button(ui, "Update extraction save selection", accepted_selection.as_ref().is_none_or(|accepted| accepted != &self.save_current)) {
                            command = Some(ExtractionCommand::Revise(operation, self.save_current.iter().copied().collect()));
                        }
                }
                if let Ok(review) = application.prefab_extraction_review(operation) {
                    ui.child_window("extraction-reviewed-outputs").size([0.0, 130.0]).build(ui, || {
                        let documents = review.documents().len();
                        let mut clipper = ListClipper::new(documents + review.outputs().len()).begin(ui);
                        while clipper.step() {
                            for index in clipper.display_start()..clipper.display_end() {
                                if index < documents {
                                    if let Some((document, disposition)) = review.documents().nth(index) { ui.text(format!("{document:?}: {disposition:?}")); }
                                } else if let Some(output) = review.outputs().nth(index - documents) {
                                    let _id = ui.push_id(index);
                                    if ui.selectable_config(format!("{} ({} bytes)", output.project_path(), output.bytes().len()))
                                        .selected(self.preview_path.as_deref() == Some(output.project_path())).build() {
                                        self.preview_path = Some(output.project_path().to_owned());
                                        self.preview_offset = 0;
                                    }
                                }
                            }
                        }
                    });
                    self.show_output_preview(ui, &review);
                    ui.text_wrapped("Confirm publishes these exact outputs progressively. Pending recovery protects interrupted publication; this is not a multi-file atomic Save.");
                    if button(ui, "Confirm reviewed extraction", !application.ordinary_input_locked()) {
                        command = Some(ExtractionCommand::Confirm(operation, frame_revision));
                    }
                }
                if button(ui, "Cancel extraction", view.can_cancel()) {
                    command = Some(ExtractionCommand::Cancel(operation));
                }
                if view.can_retry() && button(ui, "Retry extraction", true) {
                    command = Some(ExtractionCommand::Retry(operation));
                }
                if view.phase == EditorPrefabExtractionPhase::Terminal
                    && button(ui, "Dismiss extraction result", true)
                {
                    self.dismissed = Some(operation);
                    self.notice = None;
                }
            });
        command.map(|command| ExtractionAction {
            generation: application.workspace_generation(),
            command,
        })
    }

    pub(super) fn apply(
        &mut self,
        application: &mut EditorApplication,
        action: ExtractionAction,
    ) -> bool {
        if action.generation != application.workspace_generation() {
            self.notice =
                Some("The project changed before the extraction command was dispatched.".into());
            return false;
        }
        let result = match action.command {
            ExtractionCommand::Prepare { request, basis } => {
                let current = application.current_frame().is_some_and(|frame| {
                    frame
                        .workspace()
                        .document(request.document)
                        .is_some_and(|document| {
                            document.revision == basis.revision
                                && document.authoring_schema_basis == basis.schema
                        })
                });
                if !current {
                    Err(EditorPrefabExtractionError::Stale)
                } else {
                    application.begin_prefab_extraction(request).map(|_| {
                        self.draft = None;
                    })
                }
            }
            ExtractionCommand::Revise(operation, selected) => {
                application.revise_prefab_extraction(operation, selected)
            }
            ExtractionCommand::Confirm(operation, revision) => {
                if application
                    .current_frame()
                    .is_some_and(|frame| frame.revision() == revision)
                {
                    application.confirm_prefab_extraction(operation)
                } else {
                    Err(EditorPrefabExtractionError::Stale)
                }
            }
            ExtractionCommand::Cancel(operation) => application.cancel_prefab_extraction(operation),
            ExtractionCommand::Retry(operation) => application.retry_prefab_extraction(operation),
        };
        self.notice = result
            .err()
            .map(|error| format!("Extraction command refused: {error}"));
        true
    }

    fn show_output_preview(
        &mut self,
        ui: &Ui,
        review: &nara::project_host::EditorPrefabExtractionReview<'_>,
    ) {
        let Some(output) = review
            .outputs()
            .find(|output| self.preview_path.as_deref() == Some(output.project_path()))
        else {
            return;
        };
        let Ok(text) = std::str::from_utf8(output.bytes()) else {
            return;
        };
        let start = text.floor_char_boundary(self.preview_offset);
        let end = text.floor_char_boundary(start.saturating_add(16 * 1024));
        ui.text(format!("Encoded bytes {start}..{end} of {}", text.len()));
        ui.child_window("extraction-output-preview")
            .size([0.0, 150.0])
            .build(ui, || {
                ui.text_wrapped(&text[start..end]);
            });
        if button(ui, "Previous output bytes", start > 0) {
            self.preview_offset = start.saturating_sub(16 * 1024);
        }
        ui.same_line();
        if button(ui, "Next output bytes", end < text.len()) {
            self.preview_offset = end;
        }
    }
}

fn button(ui: &Ui, label: &str, enabled: bool) -> bool {
    let _disabled = ui.begin_disabled_with_cond(!enabled);
    let clicked = ui.button(label) && enabled;
    #[cfg(test)]
    super::interaction_tests::record_control(ui, label);
    clicked
}
