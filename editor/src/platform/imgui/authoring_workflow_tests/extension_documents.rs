use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use dear_imgui_rs::Ui;
use nara::{
    core::{DocumentContentState, DocumentHistoryStatus, DocumentRevision},
    diagnostic::{DiagnosticCode, SafeSummary},
    render::DisplayRect,
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId,
        EditorExtensionCommand, EditorExtensionDocument, EditorExtensionDocumentError,
        EditorPersistenceAction, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
        EditorWorkspaceIntentResult,
    },
};

use super::{
    ActiveEventLoop, ApplicationHandler, AuthoringWorkflow, Duration, EditorDocumentKind,
    EditorPersistenceResult, EventLoop, EventLoopBuilderExtWindows, Instant, InteractionProject,
    Key, MouseButton, Path, PathBuf, WindowEvent, WindowId, fs, read_source, run_native_workflow,
};
use crate::{
    EditorNativeComposition, ImguiEditorPanel, ImguiEditorPanelContext, PanelActionDispatch,
    PanelSubmissionId,
    workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelSourceKind, EditorPanelTarget,
    },
};

const SOURCE: &str = "native-dialogue.json";
const PANEL: &str = "qualification.dialogue";
const LINE_LIMIT: usize = 1024;
const HISTORY_LIMIT: usize = 8;

struct SetOpeningLine(String);

#[derive(Debug)]
struct DialogueView {
    line: String,
}

type Snapshot = (DocumentContentState, String);

struct Dialogue {
    line: String,
    clock: DocumentRevision,
    state: DocumentContentState,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

fn document_error() -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new("qualification.dialogue-invalid").expect("static diagnostic code"),
        SafeSummary::new("the bounded dialogue operation was refused").expect("static summary"),
    )
}

impl EditorExtensionDocument for Dialogue {
    type Command = SetOpeningLine;
    type View = DialogueView;
    const TYPE_ID: &'static str = "qualification.dialogue";

    fn title(&self) -> &str {
        "Native dialogue"
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

    fn view(&self) -> DialogueView {
        DialogueView {
            line: self.line.clone(),
        }
    }

    fn apply(&mut self, command: &SetOpeningLine) -> Result<(), EditorExtensionDocumentError> {
        if command.0.len() > LINE_LIMIT {
            return Err(document_error());
        }
        self.undo.push((self.state, self.line.clone()));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.line.clone_from(&command.0);
        self.clock = self.clock.next();
        self.state = self.clock.content_state();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, line) = self.undo.pop().ok_or_else(document_error)?;
        self.redo
            .push((self.state, std::mem::replace(&mut self.line, line)));
        self.state = state;
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        let (state, line) = self.redo.pop().ok_or_else(document_error)?;
        self.undo
            .push((self.state, std::mem::replace(&mut self.line, line)));
        self.state = state;
        Ok(())
    }

    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        serde_json::to_vec(&self.line).map_err(|_| document_error())
    }

    fn decode(bytes: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        if bytes.len() > LINE_LIMIT * 6 + 2 {
            return Err(document_error());
        }
        let line: String = serde_json::from_slice(bytes).map_err(|_| document_error())?;
        if line.len() > LINE_LIMIT {
            return Err(document_error());
        }
        let clock = DocumentRevision::initial();
        Ok(Self {
            line,
            clock,
            state: clock.content_state(),
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }
}

#[derive(Default)]
struct PanelEvidence {
    created: usize,
    dropped: Vec<(usize, String)>,
    last: Option<(usize, String, String)>,
    controls: BTreeMap<&'static str, DisplayRect>,
    submissions: Vec<PanelSubmissionId>,
    completions: Vec<(PanelSubmissionId, PanelActionDispatch)>,
    error: Option<String>,
}

struct DialoguePanel {
    serial: usize,
    draft: String,
    basis: Option<DocumentRevision>,
    evidence: Rc<RefCell<PanelEvidence>>,
}

impl DialoguePanel {
    fn record(&self, ui: &Ui, label: &'static str) {
        if ui.is_item_visible() {
            let (min, max) = ui.item_rect();
            self.evidence
                .borrow_mut()
                .controls
                .insert(label, DisplayRect { min, max });
        }
    }
}

impl ImguiEditorPanel for DialoguePanel {
    fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        self.evidence.borrow_mut().controls.clear();
        let model = context.model();
        let Some(projection) = model.extension_document() else {
            self.evidence.borrow_mut().error =
                Some("the native panel has no extension projection".to_owned());
            return;
        };
        let Some(view) = projection.get::<Dialogue>() else {
            self.evidence.borrow_mut().error =
                Some("the native panel lost its typed dialogue view".to_owned());
            return;
        };
        let document = projection.document();
        let revision = projection.revision();
        if self.basis.is_none() {
            self.draft.clone_from(&view.line);
            self.basis = Some(revision);
        }
        ui.text(format!("Opening line: {}", view.line));
        ui.input_text("Opening line draft", &mut self.draft).build();
        self.record(ui, "draft");
        let reload = ui.button("Reload opening line");
        self.record(ui, "reload");
        if reload {
            self.draft.clone_from(&view.line);
            self.basis = Some(revision);
        }
        let applied = ui.with_disabled_if(self.basis != Some(revision), || {
            ui.button("Apply opening line")
        });
        self.record(ui, "apply");
        self.evidence.borrow_mut().last =
            Some((self.serial, self.draft.clone(), view.line.clone()));
        if applied {
            let action = EditorAction::Extension(EditorExtensionCommand::new::<Dialogue>(
                document,
                revision,
                SetOpeningLine(self.draft.clone()),
            ));
            match context.submit(action) {
                Ok(id) => self.evidence.borrow_mut().submissions.push(id),
                Err(error) => self.evidence.borrow_mut().error = Some(error.to_string()),
            }
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        self.evidence.borrow_mut().completions.push((id, result));
    }
}

impl Drop for DialoguePanel {
    fn drop(&mut self) {
        self.evidence
            .borrow_mut()
            .dropped
            .push((self.serial, self.draft.clone()));
    }
}

type DialogueState = (String, usize, usize, bool);

struct ExtensionObservation {
    states: Vec<DialogueState>,
    sources: Vec<serde_json::Value>,
    frames: u64,
    presents: u64,
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 adapter"]
fn native_extension_document_widgets_save_reopen_and_retire_panels() {
    let project = InteractionProject::new();
    fs::write(project.root.join(SOURCE), br#""Welcome""#).unwrap();
    let evidence = Rc::new(RefCell::new(PanelEvidence::default()));
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = ExtensionTestApplication {
        project: project.root.clone(),
        evidence: Rc::clone(&evidence),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // The harness retires graphics and the Session on every returned error before assertions.
    let observed = application
        .result
        .expect("the native extension workflow must run")
        .unwrap();
    let expected = [
        ("Welcome", 0, 0, false),
        ("Welcome", 0, 0, false),
        ("Edited opening", 1, 0, true),
        ("Welcome", 0, 1, false),
        ("Edited opening", 1, 0, true),
        ("Edited opening", 1, 0, true),
        ("Edited opening", 1, 0, false),
        ("Welcome", 0, 1, true),
        ("Edited opening", 1, 0, false),
        ("Edited opening", 0, 0, false),
        ("Discard this edit", 1, 0, true),
        ("Discard this edit", 1, 0, true),
        ("Edited opening", 0, 0, false),
        ("Saved on close", 1, 0, true),
        ("Saved on close", 0, 0, false),
    ]
    .map(|(line, undo, redo, dirty)| (line.to_owned(), undo, redo, dirty));
    assert_eq!(observed.states, expected);
    assert_eq!(
        observed.sources,
        ["Edited opening", "Edited opening", "Saved on close"].map(serde_json::Value::from)
    );
    assert!(observed.frames > 0);
    assert_eq!(
        observed.presents, observed.frames,
        "every measured native frame must present exactly once"
    );
    let evidence = evidence.borrow();
    assert_eq!(evidence.error, None);
    assert_eq!(evidence.created, 4);
    assert_eq!(
        evidence.dropped,
        [
            (1, "Retired draft only".to_owned()),
            (2, "Discard this edit".to_owned()),
            (3, "Saved on close".to_owned()),
            (4, "Saved on close".to_owned()),
        ]
    );
    assert_eq!(evidence.submissions.len(), 3);
    assert_eq!(evidence.completions.len(), 3);
    for (submitted, (finished, result)) in evidence.submissions.iter().zip(&evidence.completions) {
        assert_eq!(submitted, finished);
        assert!(matches!(
            result,
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied
        ));
    }
}

struct ExtensionTestApplication {
    project: PathBuf,
    evidence: Rc<RefCell<PanelEvidence>>,
    result: Option<Result<ExtensionObservation, String>>,
}

impl ApplicationHandler for ExtensionTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_extension_documents(&self.project, Rc::clone(&self.evidence))
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn dialogue_state(&self, document: EditorDocumentId) -> Result<DialogueState, String> {
        let frame = self.frame()?;
        let tab = frame
            .workspace()
            .document(document)
            .ok_or("the dialogue tab disappeared")?;
        let view = frame
            .workspace()
            .extension_document(document)
            .and_then(|projection| projection.get::<Dialogue>())
            .ok_or("the dialogue typed projection disappeared")?;
        if frame.workspace().active_document != Some(document)
            || tab.kind != EditorDocumentKind::Extension(Dialogue::TYPE_ID)
            || tab.source_binding.as_ref().map(|source| source.locator()) != Some(SOURCE)
        {
            return Err(
                "the independent dialogue lost its active document/source identity".to_owned(),
            );
        }
        Ok((
            view.line.clone(),
            tab.history.undo_depth,
            tab.history.redo_depth,
            tab.dirty,
        ))
    }

    fn click_dialogue(
        &mut self,
        evidence: &RefCell<PanelEvidence>,
        label: &str,
    ) -> Result<(), String> {
        let rect = evidence
            .borrow()
            .controls
            .get(label)
            .copied()
            .ok_or_else(|| format!("the native dialogue {label} widget was not visible"))?;
        self.pointer = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        Ok(())
    }

    fn type_dialogue(
        &mut self,
        evidence: &RefCell<PanelEvidence>,
        text: &str,
    ) -> Result<(), String> {
        self.click_dialogue(evidence, "draft")?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::ModCtrl, true);
            context.io_mut().add_key_event(Key::A, true);
        })?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::A, false);
            context.io_mut().add_key_event(Key::ModCtrl, false);
        })?;
        self.paint(|context| context.io_mut().add_input_characters_utf8(text))?;
        if evidence
            .borrow()
            .last
            .as_ref()
            .map(|(_, draft, _)| draft.as_str())
            != Some(text)
        {
            return Err("Context input did not replace the native dialogue draft".to_owned());
        }
        Ok(())
    }

    fn wait_dialogue_panel(
        &mut self,
        evidence: &RefCell<PanelEvidence>,
        serial: usize,
        line: &str,
    ) -> Result<(), String> {
        evidence.borrow_mut().last = None;
        for _ in 0..8 {
            self.paint(|_| {})?;
            if evidence
                .borrow()
                .last
                .as_ref()
                .is_some_and(|(actual, draft, view)| {
                    *actual == serial && draft == line && view == line
                })
            {
                return Ok(());
            }
        }
        Err(format!(
            "native dialogue panel {serial} did not paint a fresh {line:?} draft"
        ))
    }

    fn request_dialogue_close(&mut self, document: EditorDocumentId) -> Result<(), String> {
        // Close Document has no recorded widget rectangle; only the request uses public ingress.
        // The decision below still travels through the actual native Unsaved changes buttons.
        let frame = self.frame()?;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                        document,
                    }),
                )],
            ),
            Duration::ZERO,
        );
        self.paint(|_| {})?;
        if self
            .frame()?
            .workspace_intent()
            .decision_prompt()
            .map(|prompt| prompt.document())
            != Some(document)
        {
            return Err(
                "the public close request did not paint the exact dialogue decision".to_owned(),
            );
        }
        Ok(())
    }

    fn acknowledge_dialogue_close(&mut self) -> Result<(), String> {
        let frame = self.frame()?;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::AcknowledgeResult,
                )],
            ),
            Duration::ZERO,
        );
        self.paint(|_| {})
    }

    fn wait_dialogue_closed(&mut self, document: EditorDocumentId) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            let frame = self.frame()?;
            if frame.workspace().document(document).is_none()
                && frame.workspace().extension_document(document).is_none()
                && frame.workspace_intent().result()
                    == Some(EditorWorkspaceIntentResult::Applied {
                        intent: EditorWorkspaceIntent::CloseDocument { document },
                    })
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("the native close decision did not retire the exact dialogue document".to_owned())
    }

    fn run_extension_documents(
        mut self,
        project: &Path,
        evidence: Rc<RefCell<PanelEvidence>>,
    ) -> Result<ExtensionObservation, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.application.current_frame().is_none() && Instant::now() < deadline {
            self.paint(|_| {})?;
            std::thread::sleep(Duration::from_millis(1));
        }
        let scene = self.frame()?.workspace().documents.clone();
        self.application
            .register_extension_document::<Dialogue>()
            .map_err(|error| error.to_string())?;
        let document = self
            .application
            .open_extension_document::<Dialogue>(SOURCE)
            .map_err(|error| error.to_string())?;
        let mut composition = EditorNativeComposition::new();
        let shared = Rc::clone(&evidence);
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    PANEL,
                    "Native dialogue",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Center,
                    true,
                ),
                move || {
                    let serial = {
                        let mut observed = shared.borrow_mut();
                        observed.created += 1;
                        observed.created
                    };
                    DialoguePanel {
                        serial,
                        draft: String::new(),
                        basis: None,
                        evidence: Rc::clone(&shared),
                    }
                },
            )
            .map_err(|error| error.to_string())?;
        self.native.frontend = composition
            .install_panels(self.application)
            .map_err(|error| error.to_string())?;
        let instances = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .to_vec();
        let instance = instances
            .iter()
            .find(|instance| instance.kind() == PANEL)
            .ok_or("the native dialogue panel was not installed")?
            .id();
        // Give the custom panel the central dock; retain the production toolbar and close window.
        for builtin in instances.iter().filter(|instance| instance.kind() != PANEL) {
            self.application
                .panel_workspace_mut()
                .layout_mut()
                .set_visible(builtin.id(), false);
        }
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_target(
                instance,
                EditorPanelTarget::Source {
                    kind: EditorPanelSourceKind::Extension,
                    locator: SOURCE.to_owned(),
                },
            );
        self.native.frontend.reset_layout();
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.wait_dialogue_panel(&evidence, 1, "Welcome")?;
        let initial_frame = self.native.frame_index;
        let initial_presents = self.presents;
        let mut states = vec![self.dialogue_state(document)?];
        self.type_dialogue(&evidence, "Edited opening")?;
        states.push(self.dialogue_state(document)?);
        self.click_dialogue(&evidence, "apply")?;
        states.push(self.dialogue_state(document)?);
        self.click("Undo")?;
        states.push(self.dialogue_state(document)?);
        self.click("Redo")?;
        states.push(self.dialogue_state(document)?);

        self.type_dialogue(&evidence, "Retired draft only")?;
        let before = self.frame()?.workspace().document(document).cloned();
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(instance, false);
        self.paint(|_| {})?;
        if evidence.borrow().dropped != [(1, "Retired draft only".to_owned())]
            || self.frame()?.workspace().document(document).cloned() != before
        {
            return Err("hiding the native panel did not retire only its draft".to_owned());
        }
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(instance, true);
        self.wait_dialogue_panel(&evidence, 2, "Edited opening")?;
        if self.frame()?.workspace().document(document).cloned() != before {
            return Err(
                "showing the native panel changed its retained document/history".to_owned(),
            );
        }
        states.push(self.dialogue_state(document)?);

        let saved_revision = self
            .frame()?
            .workspace()
            .document(document)
            .ok_or("missing dialogue")?
            .revision;
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { document: actual, revision, .. })
                if actual == document && revision == saved_revision
        ) {
            return Err(
                "the native Save widget did not publish the exact dialogue revision".to_owned(),
            );
        }
        states.push(self.dialogue_state(document)?);
        let mut sources = vec![read_source(&project.join(SOURCE))?];
        self.click("Dismiss persistence result")?;
        self.click("Undo")?;
        states.push(self.dialogue_state(document)?);
        self.click("Redo")?;
        states.push(self.dialogue_state(document)?);
        self.click("Reopen")?;
        self.settle_persistence()?;
        let reopened = self.frame()?;
        let reopened_revision = reopened
            .workspace()
            .document(document)
            .ok_or("Reopen lost the dialogue")?
            .revision;
        if !matches!(
            reopened.persistence().result(),
            Some(EditorPersistenceResult::Opened { document: actual, revision, .. })
                if actual == document && revision == reopened_revision
        ) || reopened_revision.source_id() == saved_revision.source_id()
        {
            return Err(
                "the native Reopen widget did not install a fresh dialogue incarnation".to_owned(),
            );
        }
        states.push(self.dialogue_state(document)?);
        self.click("Dismiss persistence result")?;
        self.click_dialogue(&evidence, "reload")?;
        self.type_dialogue(&evidence, "Discard this edit")?;
        self.click_dialogue(&evidence, "apply")?;
        states.push(self.dialogue_state(document)?);
        let before_cancel = self.frame()?.workspace().document(document).cloned();
        self.request_dialogue_close(document)?;
        self.click("close:Cancel")?;
        if self.frame()?.workspace().document(document).cloned() != before_cancel
            || self.frame()?.workspace_intent().result()
                != Some(EditorWorkspaceIntentResult::Cancelled {
                    intent: EditorWorkspaceIntent::CloseDocument { document },
                })
        {
            return Err("the native Cancel decision changed the dirty dialogue".to_owned());
        }
        states.push(self.dialogue_state(document)?);
        self.acknowledge_dialogue_close()?;
        self.request_dialogue_close(document)?;
        self.click("close:Discard and continue")?;
        self.wait_dialogue_closed(document)?;
        sources.push(read_source(&project.join(SOURCE))?);
        self.acknowledge_dialogue_close()?;

        let document = self
            .application
            .open_extension_document::<Dialogue>(SOURCE)
            .map_err(|error| error.to_string())?;
        self.wait_dialogue_panel(&evidence, 3, "Edited opening")?;
        states.push(self.dialogue_state(document)?);
        self.type_dialogue(&evidence, "Saved on close")?;
        self.click_dialogue(&evidence, "apply")?;
        states.push(self.dialogue_state(document)?);
        let close_revision = self
            .frame()?
            .workspace()
            .document(document)
            .ok_or("missing close dialogue")?
            .revision;
        self.request_dialogue_close(document)?;
        self.click("close:Save and continue")?;
        self.settle_persistence()?;
        self.wait_dialogue_closed(document)?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { document: actual, revision, .. })
                if actual == document && revision == close_revision
        ) {
            return Err(
                "the native Save close decision did not publish the captured dialogue".to_owned(),
            );
        }
        sources.push(read_source(&project.join(SOURCE))?);
        self.acknowledge_dialogue_close()?;
        let frame = self.frame()?;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult {
                        attempt: frame
                            .persistence()
                            .attempt()
                            .expect("the observed persistence result is terminal"),
                    },
                )],
            ),
            Duration::ZERO,
        );
        let document = self
            .application
            .open_extension_document::<Dialogue>(SOURCE)
            .map_err(|error| error.to_string())?;
        self.wait_dialogue_panel(&evidence, 4, "Saved on close")?;
        states.push(self.dialogue_state(document)?);
        for original in &scene {
            let mut expected = original.clone();
            expected.active = false;
            if self.frame()?.workspace().document(original.document) != Some(&expected) {
                return Err(
                    "the independent document workflow changed an ordinary source tab".to_owned(),
                );
            }
        }
        Ok(ExtensionObservation {
            states,
            sources,
            frames: self.native.frame_index - initial_frame,
            presents: self.presents - initial_presents,
        })
    }
}
