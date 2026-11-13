use super::*;
use nara_engine::project_host::{EditorProjectOpenHost, EditorProjectOpenProgress};
use nara_engine::tooling::{
    EditorExtensionDocument, EditorPersistenceAction, EditorPersistenceAttemptId,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorPersistenceView,
};
use nara_gpui_engine_consumer::dialogue::{
    ChoiceDraft, DialogueCommand, DialogueCommandOutcome, DialogueGraph, NodeBody, NodeRecipe,
    OutputPort, workflow::DialogueSnapshot,
};

const SOURCE: &str = "dialogue/conversation.json";

pub(super) fn open_authoring_session(root: DirectoryCapability) -> Result<EditorProjectSession> {
    let mut host = EditorProjectOpenHost::new().with_recipe(ProductRecipe::new());
    let mut attempt = host.begin(root, None);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Pending(next) => attempt = next,
            EditorProjectOpenProgress::Opened(session) => return Ok(session),
            EditorProjectOpenProgress::Rejected(mut failure) => {
                while failure.advance_retirement() != RuntimeCandidateRetirementState::Retired {
                    ensure!(
                        Instant::now() < deadline,
                        "authoring Open cleanup exceeded deadline"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                bail!(
                    "Dialogue authoring Open failed: {:?}",
                    failure.diagnostics()
                );
            }
        }
        ensure!(
            Instant::now() < deadline,
            "Dialogue authoring Open exceeded deadline"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(crate) struct DialogueDocument {
    pub document: EditorDocumentId,
    pub status: String,
    attempt: Option<EditorPersistenceAttemptId>,
    browser_result: Option<nara_engine::tooling::EditorDocumentOpenResult>,
}

impl Engine {
    pub fn open_dialogue(&mut self) -> Result<DialogueSnapshot> {
        ensure!(
            self.dialogue.is_none(),
            "the Dialogue document is already open"
        );
        fs::create_dir_all(self._project.path().join("dialogue"))?;
        fs::write(
            self._project.path().join(SOURCE),
            DialogueGraph::new().encode()?,
        )?;
        self.session
            .register_extension_document::<DialogueGraph>()?;
        let document = self
            .session
            .open_extension_document::<DialogueGraph>(SOURCE)?;
        self.session.drive_editor_frame(Duration::ZERO);
        let _ = self.dialogue_action(EditorAction::ProjectBrowser(
            nara_engine::tooling::EditorProjectBrowserAction::Refresh,
        ))?;
        self.dialogue = Some(DialogueDocument {
            document,
            status: "Ready".into(),
            attempt: None,
            browser_result: None,
        });
        let mut nodes = Vec::new();
        for (recipe, position) in [
            (NodeRecipe::Line, [24., 40.]),
            (NodeRecipe::Choice, [340., 40.]),
            (NodeRecipe::End, [660., 40.]),
        ] {
            let command = self
                .dialogue_snapshot()?
                .command(DialogueCommand::Add { recipe, position });
            let outcome = self.dialogue_action(EditorAction::Extension(command))?;
            let EditorActionOutcome::Workspace(report) = outcome else {
                bail!("Dialogue initialization did not return its original command result");
            };
            let receipt = report
                .extension_outcome
                .context("missing Dialogue creation receipt")?;
            let DialogueCommandOutcome::Created { node, .. } = receipt
                .get::<DialogueGraph>()
                .context("foreign Dialogue creation receipt")?
                .outcome()
            else {
                bail!("Dialogue initialization did not allocate a node");
            };
            nodes.push(*node);
        }
        let [line, choice, ending] = nodes.as_slice() else {
            bail!("Dialogue fixture requires three nodes");
        };
        let snapshot = self.dialogue_snapshot()?;
        let NodeBody::Choice { choices, .. } = snapshot.view.node(*choice).unwrap().body.as_ref()
        else {
            bail!("the Dialogue choice recipe changed");
        };
        let choice_id = choices[0].id;
        for command in [
            DialogueCommand::SetLine {
                node: *line,
                speaker: "Guide / 向导".into(),
                text: "Welcome, traveler.\n欢迎来到森林。".into(),
            },
            DialogueCommand::SetTarget {
                port: OutputPort::Next { node: *line },
                target: Some(*choice),
            },
            DialogueCommand::SetChoices {
                node: *choice,
                prompt: "Where will you go?\n请选择下一步。".into(),
                choices: vec![
                    ChoiceDraft {
                        id: Some(choice_id),
                        label: "Enter the forest / 森林".into(),
                        target: Some(*ending),
                    },
                    ChoiceDraft {
                        id: None,
                        label: "Ask again / 再问一次".into(),
                        target: Some(*line),
                    },
                ],
            },
            DialogueCommand::SetEnd {
                node: *ending,
                text: "You reached the forest.\n旅程结束。".into(),
            },
        ] {
            let command = self.dialogue_snapshot()?.command(command);
            let outcome = self.dialogue_action(EditorAction::Extension(command))?;
            ensure!(
                matches!(outcome, EditorActionOutcome::Workspace(ref report) if report.applied),
                "Dialogue seed was refused"
            );
        }
        self.dialogue_snapshot()
    }

    pub fn dialogue_snapshot(&self) -> Result<DialogueSnapshot> {
        let document = self
            .dialogue
            .as_ref()
            .context("Dialogue is not open")?
            .document;
        DialogueSnapshot::capture(&self.session, document)
            .context("the original Dialogue owner is unavailable")
    }

    pub fn dialogue_browser(&self) -> nara_engine::tooling::EditorProjectBrowserView {
        self.session.current_frame().project_browser().clone()
    }

    pub fn dialogue_browser_result(
        &self,
    ) -> Option<nara_engine::tooling::EditorDocumentOpenResult> {
        self.dialogue
            .as_ref()
            .and_then(|dialogue| dialogue.browser_result)
    }

    pub fn dialogue_work_pending(&self) -> bool {
        let frame = self.session.current_frame();
        frame.project_browser().has_pending_work()
            || frame.document_open().pending().is_some()
            || self
                .dialogue_snapshot()
                .is_ok_and(|snapshot| snapshot.persistence_pending)
    }

    pub fn dialogue_open_resource(
        &mut self,
        generation: u64,
        path: &str,
    ) -> Result<(String, Option<nara_engine::tooling::EditorDocumentOpenId>)> {
        let browser = self.dialogue_browser();
        if browser.snapshot_generation() != Some(generation) {
            return Ok((
                "Source index changed; select the resource again".into(),
                None,
            ));
        }
        let Some(entry) = browser.entry(path) else {
            return Ok(("Resource is no longer indexed".into(), None));
        };
        if !entry.open_capability().is_document() {
            return Ok((
                "This resource is read only and cannot be opened here".into(),
                None,
            ));
        }
        let outcome = self.dialogue_action(EditorAction::ProjectBrowser(
            nara_engine::tooling::EditorProjectBrowserAction::Open {
                relative_path: Arc::from(path),
            },
        ))?;
        use nara_engine::tooling::EditorProjectBrowserActionOutcome;
        println!("NARA_GPUI_RESOURCE_OPEN {outcome:?}");
        match outcome {
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
                request,
            }) => Ok(("Opening selected document".into(), Some(request))),
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Activated {
                ..
            }) => Ok(("Document activated".into(), None)),
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Rejected {
                ..
            }) => Ok(("Document could not be opened".into(), None)),
            _ => Ok(("Document could not be opened".into(), None)),
        }
    }

    pub fn dialogue_action(&mut self, action: EditorAction) -> Result<EditorActionOutcome> {
        ensure!(!self.closing, "the original Host is closing");
        let report = dispatch_without_drive(&mut self.session, action)?;
        let mut outcomes = report.into_outcomes();
        ensure!(
            outcomes.len() == 1,
            "Dialogue lost its original result slot"
        );
        let outcome = outcomes.remove(0);
        self.session.drive_editor_frame(Duration::ZERO);
        if let Some(dialogue) = &mut self.dialogue {
            dialogue.status = super::super::dialogue_panel::result_status(&outcome).into();
        }
        if let EditorActionOutcome::Persistence(result) = &outcome {
            match result {
                EditorPersistenceRequestResult::Accepted { attempt }
                | EditorPersistenceRequestResult::Rejected { attempt, .. } => {
                    let dialogue = self.dialogue.as_mut().context("Dialogue is not open")?;
                    dialogue.attempt = Some(*attempt);
                }
                EditorPersistenceRequestResult::Refused(_)
                | EditorPersistenceRequestResult::Acknowledged { .. } => {}
            }
        }
        Ok(outcome)
    }

    pub fn poll_dialogue(&mut self) -> Result<DialogueSnapshot> {
        self.session.drive_editor_frame(Duration::ZERO);
        if let Some(result) = self.session.current_frame().document_open().result() {
            println!("NARA_GPUI_RESOURCE_OPEN_RESULT {result:?}");
            let dialogue = self.dialogue.as_mut().context("Dialogue is not open")?;
            dialogue.status = resource_open_status(result);
            dialogue.browser_result = Some(result);
            dispatch_without_drive(
                &mut self.session,
                EditorAction::ProjectBrowser(
                    nara_engine::tooling::EditorProjectBrowserAction::AcknowledgeOpen {
                        request: result.request(),
                    },
                ),
            )?;
            self.session.drive_editor_frame(Duration::ZERO);
        }
        let persistence = self.session.current_frame().persistence();
        if let EditorPersistenceView::Terminal { attempt, result } = persistence
            && self
                .dialogue
                .as_ref()
                .is_some_and(|dialogue| dialogue.attempt == Some(attempt))
        {
            let dialogue = self.dialogue.as_mut().unwrap();
            dialogue.status = match result {
                EditorPersistenceResult::Saved { .. } => "Saved".into(),
                EditorPersistenceResult::Opened { .. } => "Reopened; old drafts retired".into(),
                _ => "Persistence failed; document and drafts remain available".into(),
            };
            dispatch_without_drive(
                &mut self.session,
                EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
            )?;
            self.session.drive_editor_frame(Duration::ZERO);
            self.dialogue.as_mut().unwrap().attempt = None;
        }
        self.dialogue_snapshot()
    }

    pub fn dialogue_saved_view(
        &self,
    ) -> Result<Arc<nara_gpui_engine_consumer::dialogue::DialogueView>> {
        Ok(DialogueGraph::decode(&fs::read(self._project.path().join(SOURCE))?)?.view())
    }
}

pub(crate) fn resource_open_status(
    result: nara_engine::tooling::EditorDocumentOpenResult,
) -> String {
    use nara_engine::tooling::EditorDocumentOpenResult;
    match result {
        EditorDocumentOpenResult::Opened { .. } => "Document opened".into(),
        EditorDocumentOpenResult::Cancelled { .. } => "Open cancelled".into(),
        EditorDocumentOpenResult::Rejected { .. } => "Document could not be opened".into(),
    }
}
