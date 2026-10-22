//! A small independent document owner with typed edits, bounded history, and a JSON codec.

use std::collections::VecDeque;

use nara::{
    core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision},
    diagnostic::{DiagnosticCode, SafeSummary},
};
use nara_tooling::{
    EditorAction, EditorExtensionCommand, EditorExtensionDocument, EditorExtensionDocumentError,
};

const HISTORY_LIMIT: usize = 64;
const CONTENT_LIMIT: usize = 64 * 1024;
pub const PANEL_KIND: &str = "consumer.dialogue";

pub struct SetLine {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueView {
    pub lines: Vec<String>,
}

type Snapshot = (DocumentContentState, Vec<String>);

pub struct Dialogue {
    lines: Vec<String>,
    clock: DocumentRevision,
    state: DocumentContentState,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
}

fn error(code: &'static str, summary: &'static str) -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new(code).expect("static diagnostic code"),
        SafeSummary::new(summary).expect("static safe summary"),
    )
}

impl EditorExtensionDocument for Dialogue {
    type Command = SetLine;
    type View = DialogueView;
    const TYPE_ID: &'static str = "consumer.dialogue";

    fn title(&self) -> &str {
        "Dialogue"
    }
    fn content_state(&self) -> DocumentContentState {
        self.state
    }
    fn history_status(&self) -> DocumentHistoryStatus {
        DocumentHistoryStatus {
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }
    fn view(&self) -> Self::View {
        DialogueView {
            lines: self.lines.clone(),
        }
    }

    fn apply(&mut self, command: &SetLine) -> Result<(), EditorExtensionDocumentError> {
        let Some(previous) = self.lines.get(command.index) else {
            return Err(error(
                "dialogue.line-missing",
                "the selected dialogue line does not exist",
            ));
        };
        let length = self.lines.iter().map(String::len).sum::<usize>() - previous.len();
        if command.text.len() > CONTENT_LIMIT - length {
            return Err(error(
                "dialogue.content-limit",
                "dialogue content exceeds its finite byte limit",
            ));
        }
        self.undo.push_back((self.state, self.lines.clone()));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.lines[command.index].clone_from(&command.text);
        self.clock = self.clock.next();
        self.state = self.clock.content_state();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let Some((state, lines)) = self.undo.pop_back() else {
            return Err(error(
                "dialogue.undo-empty",
                "there is no dialogue edit to undo",
            ));
        };
        self.redo
            .push((self.state, std::mem::replace(&mut self.lines, lines)));
        self.state = state;
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let Some((state, lines)) = self.redo.pop() else {
            return Err(error(
                "dialogue.redo-empty",
                "there is no dialogue edit to redo",
            ));
        };
        self.undo
            .push_back((self.state, std::mem::replace(&mut self.lines, lines)));
        self.state = state;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        serde_json::to_vec(&self.lines)
            .map_err(|_| error("dialogue.encode", "dialogue encoding failed"))
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        let lines: Vec<String> = serde_json::from_slice(bytes).map_err(|_| {
            error(
                "dialogue.decode",
                "dialogue source must be a JSON list of lines",
            )
        })?;
        if lines.is_empty()
            || lines.len() > 32
            || lines.iter().map(String::len).sum::<usize>() > CONTENT_LIMIT
        {
            return Err(error(
                "dialogue.content-limit",
                "dialogue content exceeds its finite limits",
            ));
        }
        let clock = DocumentRevision::initial();
        Ok(Self {
            lines,
            clock,
            state: clock.content_state(),
            undo: VecDeque::new(),
            redo: Vec::new(),
        })
    }
}

#[derive(Default)]
pub struct DialoguePanel {
    draft: String,
    basis: Option<DocumentRevision>,
}

impl nara_editor::ImguiEditorPanel for DialoguePanel {
    fn show(
        &mut self,
        ui: &dear_imgui_rs::Ui,
        context: &mut nara_editor::ImguiEditorPanelContext<'_, '_>,
    ) {
        let model = context.model();
        let Some(projection) = model.extension_document() else {
            return;
        };
        let Some(view) = projection.get::<Dialogue>() else {
            return;
        };
        if self.basis.is_none() {
            self.draft.clone_from(&view.lines[0]);
            self.basis = Some(projection.revision());
        }
        let document = projection.document();
        let revision = projection.revision();
        ui.input_text("Opening line", &mut self.draft).build();
        if self.basis != Some(revision) {
            ui.text("The document changed. Reload the draft before applying.");
            if ui.button("Reload draft") {
                self.draft.clone_from(&view.lines[0]);
                self.basis = Some(revision);
            }
        } else if ui.button("Apply line") {
            let action = EditorAction::Extension(EditorExtensionCommand::new::<Dialogue>(
                document,
                revision,
                SetLine {
                    index: 0,
                    text: self.draft.clone(),
                },
            ));
            let _ = context.submit(action);
        }
    }
}
