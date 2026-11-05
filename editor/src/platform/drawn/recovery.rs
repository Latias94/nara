use nara::tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorWorkspaceIntentAction,
    EditorWorkspaceIntentView,
};
use winit::{
    event::{ElementState, Ime, WindowEvent},
    keyboard::{Key, NamedKey},
};

use super::*;

pub(super) struct Recovery {
    message: String,
    decision: Option<EditorWorkspaceIntentView>,
    retry_requested: bool,
    #[cfg(test)]
    fail_next_graphics_init: bool,
}

impl DrawnNative {
    pub(super) fn pause(&mut self, message: String) {
        self.native.invalidate();
        if self.graphics.is_some() {
            if let Err(error) = self.dispatch(None) {
                self.cleanup_errors
                    .push(format!("Native input withdrawal: {error}"));
            }
        }
        self.recovery = Some(Recovery {
            message,
            decision: None,
            retry_requested: false,
            #[cfg(test)]
            fail_next_graphics_init: false,
        });
        self.refresh_recovery();
    }

    pub(super) fn refresh_recovery(&mut self) {
        let Some(recovery) = self.recovery.as_mut() else {
            return;
        };
        recovery.decision = self
            .application
            .current_frame()
            .map(|frame| frame.workspace_intent().clone());
        let control = if recovery
            .decision
            .as_ref()
            .is_some_and(|decision| decision.decision_prompt().is_some())
        {
            "S: save and continue | D: discard and continue | Escape: cancel close"
        } else if recovery
            .decision
            .as_ref()
            .is_some_and(|decision| decision.result().is_some())
        {
            "Enter: acknowledge close result | F5: retry graphics"
        } else if recovery.retry_requested {
            "Retiring original graphics | Close window: ordinary project close"
        } else {
            "F5: retry graphics | Close window: ordinary project close"
        };
        let message = recovery.message.chars().take(100).collect::<String>();
        self.window.set_title(&format!(
            "Nara Editor - rendering paused: {message} | {control}"
        ));
    }

    pub(super) fn recovery_event(&mut self, event: &WindowEvent) -> Result<(), String> {
        if matches!(
            event,
            WindowEvent::Focused(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::Ime(Ime::Enabled | Ime::Disabled)
        ) {
            self.dispatch(EditorPanelPlatformEvent::from_window_event(event))?;
            return Ok(());
        }
        let WindowEvent::KeyboardInput { event, .. } = event else {
            return Ok(());
        };
        if event.state != ElementState::Pressed || event.repeat {
            return Ok(());
        }
        if event.logical_key == Key::Named(NamedKey::F5) {
            return self.retry_graphics();
        }
        self.recovery_key(&event.logical_key)
    }

    pub(super) fn recovery_key(&mut self, key: &Key) -> Result<(), String> {
        let Some(expected) = self
            .recovery
            .as_ref()
            .and_then(|recovery| recovery.decision.as_ref())
        else {
            return Ok(());
        };
        let Some(frame) = self.application.current_frame() else {
            return Ok(());
        };
        if frame.workspace_intent() != *expected {
            return Ok(());
        }
        let action = if let Some(prompt) = expected.decision_prompt() {
            let decision = match key {
                Key::Character(value) if value.eq_ignore_ascii_case("s") => {
                    EditorCloseDecision::Save
                }
                Key::Character(value) if value.eq_ignore_ascii_case("d") => {
                    EditorCloseDecision::Discard
                }
                Key::Named(NamedKey::Escape) => EditorCloseDecision::Cancel,
                _ => return Ok(()),
            };
            EditorAction::WorkspaceIntent(prompt.resolve(decision))
        } else if expected.result().is_some() && *key == Key::Named(NamedKey::Enter) {
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult)
        } else {
            return Ok(());
        };
        self.application.submit_and_drive(
            EditorActionBatch::new(frame.revision(), [action]),
            Duration::ZERO,
        );
        self.refresh_recovery();
        Ok(())
    }

    pub(super) fn retry_graphics(&mut self) -> Result<(), String> {
        let recovery = self
            .recovery
            .as_mut()
            .ok_or("Graphics retry requires a paused workspace")?;
        if recovery.retry_requested {
            return self.poll_graphics_retry();
        }
        recovery.retry_requested = true;
        self.native.invalidate();
        if self.graphics.is_some() {
            if let Err(error) = self.dispatch(None) {
                self.cleanup_errors.push(format!(
                    "Native input withdrawal before graphics retry: {error}"
                ));
            }
        }
        drop(self.panels.prepare(&self.application, |panel, _| {
            if let Err(error) = panel.cancel_input() {
                self.cleanup_errors
                    .push(format!("Control input cancellation: {error}"));
            }
        }));
        self.poll_graphics_retry()
    }

    pub(super) fn graphics_retry_pending(&self) -> bool {
        self.recovery
            .as_ref()
            .is_some_and(|recovery| recovery.retry_requested)
    }

    /// Advances one explicitly requested reconstruction without releasing unfinished graphics.
    pub(super) fn poll_graphics_retry(&mut self) -> Result<(), String> {
        if !self.graphics_retry_pending() {
            return Ok(());
        }
        if let Some(graphics) = self.graphics.as_mut() {
            let report = graphics.poll_close();
            self.cleanup_errors.extend(report.errors);
            if !report.completed {
                self.refresh_recovery();
                return Ok(());
            }
        }
        drop(self.graphics.take());
        self.surfaces.clear();
        self.presented.clear();
        if let Some(recovery) = self.recovery.as_mut() {
            recovery.retry_requested = false;
        }
        let initialized = self.initialize_replacement_graphics();
        self.graphics = Some(match initialized {
            Ok(graphics) => graphics,
            Err(error) => {
                if let Some(recovery) = self.recovery.as_mut() {
                    recovery.message = error.clone();
                }
                let diagnostic = format!("Editor graphics reconstruction: {error}");
                if !self.cleanup_errors.contains(&diagnostic) {
                    self.cleanup_errors.push(diagnostic);
                }
                self.refresh_recovery();
                return Err(error);
            }
        });
        self.recovery = None;
        self.window.set_title("Nara Editor - Source workspace");
        self.window.request_redraw();
        Ok(())
    }

    fn initialize_replacement_graphics(&mut self) -> Result<graphics::Graphics, String> {
        #[cfg(test)]
        if self
            .recovery
            .as_mut()
            .is_some_and(|recovery| std::mem::take(&mut recovery.fail_next_graphics_init))
        {
            return Err(
                "Injected Editor graphics reconstruction initialization failure".to_owned(),
            );
        }
        graphics::Graphics::new(&self.window, self.fonts.clone())
    }

    #[cfg(test)]
    pub(super) fn fail_next_graphics_init_for_test(&mut self) {
        self.recovery
            .as_mut()
            .expect("graphics initialization control requires recovery")
            .fail_next_graphics_init = true;
    }
}
