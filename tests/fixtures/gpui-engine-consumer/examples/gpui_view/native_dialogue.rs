use super::*;
use dialogue_panel::DialoguePanel;
use nara_gpui_engine_consumer::dialogue::workflow::DialogueSnapshot;

pub(super) fn mount(
    snapshot: Option<(
        DialogueSnapshot,
        nara_engine::tooling::EditorProjectBrowserView,
    )>,
    queue: dialogue_panel::Queue,
    composing: Rc<Cell<bool>>,
    output: &RefCell<Option<Result<Entity<DialoguePanel>>>>,
    window: &mut open_gpui::Window,
    cx: &mut open_gpui::App,
) -> Option<Entity<DialoguePanel>> {
    snapshot.and_then(|(snapshot, browser)| {
        match DialoguePanel::new(snapshot, browser, queue, composing, window, cx) {
            Ok(panel) => {
                let panel = panel.mount(cx);
                *output.borrow_mut() = Some(Ok(panel.clone()));
                Some(panel)
            }
            Err(error) => {
                *output.borrow_mut() = Some(Err(error));
                None
            }
        }
    })
}

impl Viewer {
    pub(super) fn drive_dialogue(&mut self) -> Result<()> {
        loop {
            let work = self.dialogue_queue.borrow_mut().pop_front();
            let Some(work) = work else { break };
            let finish = {
                let mut engine = self.engine.as_ref().unwrap().borrow_mut();
                work(&mut engine)?
            };
            self.application.as_ref().unwrap().update(finish)?;
        }
        let (snapshot, browser, result, status) = {
            let mut engine = self.engine.as_ref().unwrap().borrow_mut();
            let snapshot = engine.poll_dialogue()?;
            let status = engine
                .dialogue
                .as_ref()
                .context("Dialogue owner is unavailable")?
                .status
                .clone();
            (
                snapshot,
                engine.dialogue_browser(),
                engine.dialogue_browser_result(),
                status,
            )
        };
        let panel = self.dialogue_panel.as_ref().unwrap();
        self.application.as_ref().unwrap().update(|cx| {
            let model = panel.read(cx).model.clone();
            model.update(cx, |model, cx| {
                model.publish_browser(browser, result, cx);
                model.publish(snapshot, status, cx);
            })
        });
        Ok(())
    }

    pub(super) fn advance_dialogue_smoke(&mut self) -> Result<()> {
        if !self.closing
            && let Some(mut smoke) = self.dialogue_smoke.take()
        {
            let done = smoke.advance(self)?;
            self.dialogue_smoke = Some(smoke);
            if done {
                self.closing = true;
            }
        }
        Ok(())
    }

    pub(super) fn close_dialogue_views(&self) -> Result<()> {
        if let Some(panel) = &self.dialogue_panel {
            self.application.as_ref().unwrap().update(|cx| {
                self.root.unwrap().update(cx, |_, window, cx| {
                    panel.update(cx, |panel, cx| panel.close(window, cx))
                })
            })??;
        }
        Ok(())
    }

    pub(super) fn verify_dialogue_retired(&self) -> Result<()> {
        if let Some(panel) = &self.dialogue_panel {
            self.application
                .as_ref()
                .unwrap()
                .update(|cx| panel.read(cx).verify_retired(cx))?;
            ensure!(
                self.dialogue_queue.borrow().is_empty(),
                "Dialogue work survived UI retirement"
            );
            ensure!(
                self.engine
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .dialogue_snapshot()
                    .is_ok(),
                "closing the last Dialogue View closed its document"
            );
        }
        Ok(())
    }
}
