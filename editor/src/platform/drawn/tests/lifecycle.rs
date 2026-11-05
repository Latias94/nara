use nara::tooling::EditorWorkspaceIntentPhase;

use super::*;

impl Workflow<'_> {
    pub(super) fn dirty_close_and_graphics_recovery(&mut self) -> TestResult {
        let original = self.project.read(fixture::PRESERVED)?;
        let clean = self.state()?;
        self.click(HIERARCHY, "+ Root")?;
        let dirty = self.expect_one_mutation(&clean)?;
        let selected = self.selected()?;
        self.replace_text("Filter entities", "opaqu")?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit("e".into())))?;
        self.event(EditorPanelPlatformEvent::Wheel {
            delta: crate::native::EditorPanelWheelDelta::Lines([0.0, 1.0]),
            phase: winit::event::TouchPhase::Moved,
        })?;
        self.paint()?;
        if self.state()? != dirty || self.field_value("Filter entities")? != "opaque" {
            return Err(
                "a stale wheel observation changed source or lost the current filter edit".into(),
            );
        }
        self.paused_dirty_exit_cancel(&dirty)?;
        if self.state()? != dirty
            || self.selected()? != selected
            || self.field_value("Filter entities")? != "opaque"
        {
            return Err("graphics recovery changed the dirty Session, selection or retained local controller draft".into());
        }
        self.click(HIERARCHY, "Clear filter")?;
        self.click(SHELL, "Close document")?;
        self.wait("show the shell's exact dirty-close decision", |workflow| {
            workflow.frame().is_ok_and(|frame| {
                frame.workspace_intent().phase()
                    == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
            })
        })?;
        self.expect_close_prompt(dirty.document)?;
        let revision = self
            .frame()?
            .workspace_intent()
            .decision_prompt()
            .ok_or("the dirty Close decision disappeared")?
            .document();
        self.click(SHELL, "Cancel")?;
        self.wait("finish the shell's exact Cancel decision", |workflow| {
            workflow
                .frame()
                .is_ok_and(|frame| frame.workspace_intent().result().is_some())
        })?;
        if revision != dirty.document
            || self.state()? != dirty
            || self.selected()? != selected
            || self.project.read(fixture::PRESERVED)? != original
        {
            return Err(
                "the presented Cancel control changed dirty source, history, selection or disk"
                    .into(),
            );
        }
        self.click(SHELL, "Acknowledge close result")?;
        if self.frame()?.workspace_intent().intent().is_some()
            || self.frame()?.workspace_intent().result().is_some()
        {
            return Err("the shell did not acknowledge the exact cancelled close result".into());
        }
        self.click(SHELL, "Close document")?;
        self.wait("show a fresh exact dirty-close decision", |workflow| {
            workflow.frame().is_ok_and(|frame| {
                frame.workspace_intent().phase()
                    == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
            })
        })?;
        self.expect_close_prompt(dirty.document)?;
        self.click(SHELL, "Discard and continue")?;
        self.wait(
            "retire the dirty document through the original close owner",
            |workflow| {
                workflow.frame().is_ok_and(|frame| {
                    frame.workspace().document(dirty.document).is_none()
                        && frame.workspace_intent().result().is_some()
                })
            },
        )?;
        if self.project.read(fixture::PRESERVED)? != original {
            return Err(
                "the actual Discard control wrote an unsaved dirty document to disk".into(),
            );
        }
        self.click(SHELL, "Acknowledge close result")?;
        let before_open = self.state()?;
        self.click(SHELL, &format!("Open {}", fixture::PRESERVED))?;
        self.wait(
            "refuse the retired source identity in the old Browser snapshot",
            |workflow| {
                workflow
                    .frame()
                    .is_ok_and(|frame| frame.document_open().result().is_some())
            },
        )?;
        if !matches!(
            self.frame()?.document_open().result(),
            Some(nara::tooling::EditorDocumentOpenResult::Rejected {
                code: "project.editor.document-source-identity-changed",
                ..
            })
        ) || self.state()? != before_open
            || self.project.read(fixture::PRESERVED)? != original
        {
            return Err("opening through the stale source snapshot bypassed exact source identity admission".into());
        }
        self.click(SHELL, "Acknowledge source open")?;
        self.click(SHELL, "Refresh sources")?;
        self.wait(
            "refresh saved source identities before reopening a closed document",
            |workflow| {
                workflow
                    .frame()
                    .is_ok_and(|frame| !frame.project_browser().has_pending_work())
            },
        )?;
        let reopened = self.open_source(fixture::PRESERVED)?;
        self.switch_document(reopened)?;
        let current = self.state()?;
        if current.document == dirty.document
            || current.rows != clean.rows
            || current.dirty
            || current.undo != 0
            || current.redo != 0
            || self.project.read(fixture::PRESERVED)? != original
        {
            return Err("opening after actual Discard did not restore the original saved source with a fresh clean document".into());
        }
        Ok(())
    }

    fn expect_close_prompt(&self, document: EditorDocumentId) -> TestResult {
        let frame = self.frame()?;
        let intent = frame.workspace_intent();
        if intent.decision_document() != Some(document)
            || intent
                .decision_prompt()
                .is_none_or(|prompt| prompt.document() != document)
            || !self.enabled(SHELL, "Cancel")?
            || !self.enabled(SHELL, "Discard and continue")?
        {
            return Err(
                "the actual shell did not present an enabled exact-document dirty-close decision"
                    .into(),
            );
        }
        Ok(())
    }

    fn paused_dirty_exit_cancel(&mut self, dirty: &SourceState) -> TestResult {
        let instance = self.panel_id(HIERARCHY)?;
        let token = self
            .native
            .surfaces
            .get(&instance)
            .ok_or("the dirty controller has no actual native Surface")?
            .token
            .clone();
        let surface = self.panel_frame(HIERARCHY)?.environment().surface_id();
        let old_presentations = self.presentations;
        let files = [
            fixture::SCENE,
            fixture::PREFAB,
            fixture::PRESERVED,
            fixture::UI,
        ]
        .map(|path| self.project.read(path))
        .into_iter()
        .collect::<TestResult<Vec<_>>>()?;
        self.native
            .pause("Injected native acceptance graphics failure".into());
        self.native
            .recovery_event(&WindowEvent::Ime(Ime::Disabled))?;
        self.native.recovery_event(&WindowEvent::Focused(false))?;
        self.native.recovery_event(&WindowEvent::Focused(true))?;
        if self.native.recovery.is_none()
            || self.state()? != *dirty
            || !token.is_current(&self.native.application)
        {
            return Err("pausing rendering discarded the original dirty Session or semantic controller incarnation".into());
        }
        if self.native.application.request_exit().exit_ready() {
            return Err(
                "ordinary window exit discarded dirty source without an exact user decision".into(),
            );
        }
        let deadline = (Instant::now() + OPERATION_TIMEOUT).min(self.deadline);
        while self.frame()?.workspace_intent().decision_prompt().is_none() {
            if Instant::now() >= deadline {
                return Err("the paused ordinary window close did not reach its exact dirty-source decision".into());
            }
            self.native.application.advance(Duration::ZERO);
            self.native.refresh_recovery();
            std::thread::sleep(Duration::from_millis(1));
        }
        self.native.refresh_recovery();
        if self.frame()?.workspace_intent().decision_document() != Some(dirty.document) {
            return Err("the paused window close targeted another dirty source".into());
        }
        self.native.recovery_key(&Key::Named(NamedKey::Escape))?;
        if self.state()? != *dirty || self.native.application.has_pending_exit() {
            return Err("the paused recovery Cancel did not retain the exact dirty source/history and cancel ordinary exit".into());
        }
        if self.frame()?.workspace_intent().result().is_none() {
            return Err("the paused Cancel did not produce an original close result".into());
        }
        self.native.recovery_key(&Key::Named(NamedKey::Enter))?;
        if self.frame()?.workspace_intent().result().is_some() || self.native.recovery.is_none() {
            return Err(
                "the paused recovery did not acknowledge its original Cancel result".into(),
            );
        }
        let graphics = self
            .native
            .graphics
            .as_mut()
            .ok_or("the paused original graphics owner disappeared")?;
        graphics.hold_close_for_test(2);
        graphics.fail_close_poll_for_test();
        self.native.fail_next_graphics_init_for_test();
        self.native.retry_graphics()?;
        if self.native.graphics.is_none()
            || !self.native.graphics_retry_pending()
            || self.state()? != *dirty
        {
            return Err(
                "a failed retirement poll released the original graphics owner or dirty Session"
                    .into(),
            );
        }
        for _ in 0..2 {
            self.native.poll_graphics_retry()?;
            if self.native.graphics.is_none() || !self.native.graphics_retry_pending() {
                return Err(
                    "incomplete physical close lost the retained original graphics owner".into(),
                );
            }
        }
        let deadline = Instant::now() + OPERATION_TIMEOUT;
        let failure = loop {
            if let Err(error) = self.native.poll_graphics_retry() {
                break error;
            }
            if Instant::now() >= deadline {
                return Err(
                    "the original graphics owner never reached replacement initialization".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        if failure != "Injected Editor graphics reconstruction initialization failure"
            || self.native.graphics.is_some()
            || self.native.graphics_retry_pending()
            || self.native.recovery.is_none()
            || self.state()? != *dirty
        {
            return Err("failed replacement initialization did not keep an explicitly retryable dirty workspace".into());
        }
        self.native.recovery_event(&WindowEvent::Focused(false))?;
        self.native.recovery_event(&WindowEvent::Focused(true))?;
        self.expected_cleanup_errors = vec![
            "Injected Editor graphics retirement poll failure".into(),
            "Editor graphics reconstruction: Injected Editor graphics reconstruction initialization failure".into(),
        ];
        if self.native.cleanup_errors != self.expected_cleanup_errors {
            return Err(format!(
                "recovery lost original failure evidence: {:?}",
                self.native.cleanup_errors
            ));
        }
        self.native.retry_graphics()?;
        self.wait(
            "present new original images after graphics recovery",
            |workflow| workflow.native.presented.len() == 2,
        )?;
        let replacement = self
            .native
            .surfaces
            .get(&instance)
            .ok_or("the recovered controller has no actual native Surface")?;
        if self.native.recovery.is_some()
            || replacement.token != token
            || replacement.surface == surface
            || self.state()? != *dirty
            || self.presentations <= old_presentations
            || self.field_value("Filter entities")? != "opaque"
        {
            return Err("graphics retry did not preserve the same controller and dirty Session while presenting fresh original Surfaces".into());
        }
        let position = self.point(HIERARCHY, UiSemanticRole::TextInput, "Filter entities")?;
        self.pointer(position)?;
        self.mouse(ElementState::Pressed)?;
        self.paint()?;
        self.mouse(ElementState::Released)?;
        self.paint()?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Enabled))?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit("恢复".into())))?;
        self.paint()?;
        if self.field_value("Filter entities")? != "opaque恢复" {
            return Err(
                "graphics recovery lost the IME disable acknowledgment before field refocus".into(),
            );
        }
        self.event(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::CONTROL,
        ))?;
        self.key(Key::Character("a".into()))?;
        self.event(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::empty(),
        ))?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit("opaque".into())))?;
        self.paint()?;
        if self.field_value("Filter entities")? != "opaque" || self.state()? != *dirty {
            return Err(
                "recovered text input changed the dirty source/history or lost its retained filter"
                    .into(),
            );
        }
        self.verify_two_original_composites()?;
        for (path, original) in [
            fixture::SCENE,
            fixture::PREFAB,
            fixture::PRESERVED,
            fixture::UI,
        ]
        .into_iter()
        .zip(files)
        {
            if self.project.read(path)? != original {
                return Err(format!(
                    "paused graphics recovery changed the original source file {path}"
                ));
            }
        }
        Ok(())
    }
}
