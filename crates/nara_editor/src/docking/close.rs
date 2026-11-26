#[cfg(all(test, windows))]
mod tests;

use std::{cell::Cell, path::PathBuf, rc::Rc};

use anyhow::{Context as _, Result};
use nara_core::DocumentId;
use open_gpui::{AnyWindowHandle, App, PromptButton, PromptLevel, WeakEntity};
use open_gpui_docking::{
    DockSurface,
    dockspace::{
        close::{CloseDecision, ClosePlanTarget, CloseRequestId},
        model::SurfaceId,
        runtime::DockspaceHostFrame,
    },
};

use super::{ROOT, save_layout, warn_layout};
use crate::{
    inspector::{InspectorDrafts, InspectorPanel},
    workspace::Workspace,
};

pub(super) fn keep_required_panes(
    dock: &DockSurface,
    workspace: WeakEntity<Workspace>,
    cx: &mut App,
) -> Result<()> {
    dock.set_close_handler(
        move |dock, plan, cx| {
            if !plan.phase().requires_decision() {
                return;
            }
            match dock.update(|frame| veto_content_close(frame, plan.request()), cx) {
                Ok(true) => {
                    warn(
                        &workspace,
                        "Editor panes stay open. Drag a floating tab into the main editor or use Dock Back when available.",
                        cx,
                    );
                    if let ClosePlanTarget::Surface { surface, .. } = plan.target()
                        && surface != ROOT
                    {
                        let dock = dock.clone();
                        cx.defer(move |cx| show_redock_hint(&dock, surface, cx));
                    }
                }
                Ok(false) => {}
                Err(error) => warn(&workspace, format!("Pane close: {error:#}"), cx),
            }
        },
        cx,
    )
}

pub(super) fn veto_content_close(
    frame: &mut DockspaceHostFrame<'_>,
    request: CloseRequestId,
) -> Result<bool> {
    let decision = frame.pending_close_decisions(request).next();
    if let Some(decision) = decision {
        frame.resolve_close(request, decision.token(), CloseDecision::Veto)?;
    }
    Ok(decision.is_some())
}

fn show_redock_hint(dock: &DockSurface, surface: SurfaceId, cx: &mut App) {
    let Ok(inspection) = dock.inspection() else {
        return;
    };
    let Some(mount) = inspection
        .mounts
        .iter()
        .find(|mount| mount.regions.iter().any(|region| region.surface == surface))
    else {
        return;
    };
    if let Some(window) = cx
        .windows()
        .into_iter()
        .find(|window| window.window_id().as_u64() == mount.window_id)
    {
        let _ = window.update(cx, |_, window, cx| {
            if !window.has_active_prompt() {
                let answer = window.prompt(
                    PromptLevel::Info,
                    "Redock the panes before closing this window",
                    Some("Drag each tab into the main editor window. You can then close this window if it stays open."),
                    &[PromptButton::ok("OK")],
                    cx,
                );
                cx.spawn(async move |_| {
                    let _ = answer.await;
                })
                .detach();
            }
        });
    }
}

#[derive(Clone, Default, PartialEq, open_gpui::Action)]
#[action(namespace = nara_editor, no_json, no_register)]
pub(crate) struct QuitEditor;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RootCloseState {
    Open,
    Pending,
    Closing,
}

pub(super) fn guard_editor_window(
    dock: &DockSurface,
    window: AnyWindowHandle,
    workspace: WeakEntity<Workspace>,
    inspector: WeakEntity<InspectorPanel>,
    layout_path: Option<PathBuf>,
    cx: &mut App,
) -> Result<Rc<Cell<RootCloseState>>> {
    let state = Rc::new(Cell::new(RootCloseState::Open));
    let close = RootClose {
        dock: dock.clone(),
        window,
        workspace: workspace.clone(),
        inspector,
        layout_path,
        state: state.clone(),
    };
    let on_window_close = close.clone();
    window
        .update(cx, |_, window, cx| {
            window.on_window_should_close(cx, move |_, cx| {
                on_window_close.trigger(cx);
                false
            });
        })
        .context("Could not install the editor window close handler")?;
    let on_quit_action = close.clone();
    cx.on_action(move |_: &QuitEditor, cx| on_quit_action.trigger(cx));
    #[cfg(target_os = "macos")]
    {
        use open_gpui::{KeyBinding, Menu, MenuItem};
        cx.bind_keys([KeyBinding::new("cmd-q", QuitEditor, None)]);
        cx.set_menus([
            Menu::new("Nara Editor").items([MenuItem::action("Quit Nara Editor", QuitEditor)])
        ]);
    }
    cx.on_app_quit(move |cx| {
        let job = workspace
            .update(cx, |workspace, cx| workspace.take_job(cx))
            .ok()
            .flatten();
        let background = cx.background_executor().clone();
        async move {
            if let Some(job) = job {
                background.spawn(async move { drop(job) }).await;
            }
        }
    })
    .detach();
    Ok(state)
}

#[derive(Clone)]
struct RootClose {
    dock: DockSurface,
    window: AnyWindowHandle,
    workspace: WeakEntity<Workspace>,
    inspector: WeakEntity<InspectorPanel>,
    layout_path: Option<PathBuf>,
    state: Rc<Cell<RootCloseState>>,
}

struct CloseReview {
    document: DocumentId,
    revision: u64,
    scene_dirty: bool,
    drafts: InspectorDrafts,
}

#[derive(Clone, Copy)]
enum CloseAnswer {
    ReviewDrafts,
    DiscardDrafts,
    SaveScene,
    DiscardScene,
    CloseWithoutLayout,
    Cancel,
}

impl RootClose {
    fn trigger(&self, cx: &mut App) {
        if self.state.get() == RootCloseState::Open {
            self.state.set(RootCloseState::Pending);
            let close = self.clone();
            cx.defer(move |cx| close.request(cx));
        }
    }

    fn request(self, cx: &mut App) {
        let review = self.workspace.read_with(cx, |workspace, cx| {
            let document = workspace.session.document();
            CloseReview {
                document: document.scene().id,
                revision: workspace.revision,
                scene_dirty: workspace.session.is_dirty()
                    && (document.path().is_some()
                        || !document.scene().entities.is_empty()
                        || workspace.session.can_undo()),
                drafts: self.drafts(cx),
            }
        });
        let Ok(review) = review else {
            self.shutdown(cx);
            return;
        };
        let review = Rc::new(review);
        if review.drafts.is_empty() {
            self.confirm_scene(review, cx);
        } else {
            self.prompt(
                review,
                "Review Inspector drafts before closing Nara Editor?",
                "Press Enter to submit text edits or use Apply in the Inspector. Reselect the original entities to resume a draft. Continuing discards the drafts when the editor closes.",
                &[
                    (CloseAnswer::ReviewDrafts, "Review in Inspector"),
                    (CloseAnswer::DiscardDrafts, "Discard Drafts and Continue"),
                    (CloseAnswer::Cancel, "Cancel"),
                ],
                cx,
            );
        }
    }

    fn confirm_scene(self, review: Rc<CloseReview>, cx: &mut App) {
        if !review.scene_dirty {
            self.finish(review, cx);
            return;
        }
        self.prompt(
            review,
            "Save scene changes before closing Nara Editor?",
            "Discard closes the editor and loses your unsaved scene changes.",
            &[
                (CloseAnswer::SaveScene, "Save and Close"),
                (CloseAnswer::DiscardScene, "Discard"),
                (CloseAnswer::Cancel, "Cancel"),
            ],
            cx,
        );
    }

    fn prompt(
        self,
        review: Rc<CloseReview>,
        title: &str,
        detail: &str,
        choices: &'static [(CloseAnswer, &'static str)],
        cx: &mut App,
    ) {
        let buttons: Vec<_> = choices
            .iter()
            .map(|&(answer, label)| match answer {
                CloseAnswer::ReviewDrafts | CloseAnswer::SaveScene => PromptButton::ok(label),
                CloseAnswer::DiscardDrafts
                | CloseAnswer::DiscardScene
                | CloseAnswer::CloseWithoutLayout => PromptButton::new(label),
                CloseAnswer::Cancel => PromptButton::cancel(label),
            })
            .collect();
        let result = self.window.update(cx, |_, window, cx| {
            if cfg!(target_os = "macos") {
                cx.set_prompt_builder(open_gpui::fallback_prompt_renderer);
            }
            let answer = window.prompt(PromptLevel::Warning, title, Some(detail), &buttons, cx);
            if cfg!(target_os = "macos") {
                cx.reset_prompt_builder();
            }
            answer
        });
        match result {
            Ok(answer) => cx
                .spawn(async move |cx| {
                    let answer = answer
                        .await
                        .ok()
                        .and_then(|index| choices.get(index))
                        .map_or(CloseAnswer::Cancel, |(answer, _)| *answer);
                    cx.update(|cx| self.resolve(review, answer, cx));
                })
                .detach(),
            Err(error) => {
                self.state.set(RootCloseState::Open);
                warn(
                    &self.workspace,
                    format!("Editor close dialog: {error:#}"),
                    cx,
                );
            }
        }
    }

    fn resolve(self, review: Rc<CloseReview>, answer: CloseAnswer, cx: &mut App) {
        if !matches!(answer, CloseAnswer::Cancel | CloseAnswer::ReviewDrafts)
            && let Err(error) = self
                .workspace
                .read_with(cx, |workspace, cx| self.validate(&review, workspace, cx))
                .and_then(|result| result)
        {
            self.state.set(RootCloseState::Open);
            warn(&self.workspace, error.to_string(), cx);
            return;
        }
        match answer {
            CloseAnswer::Cancel => self.state.set(RootCloseState::Open),
            CloseAnswer::ReviewDrafts => self.review_drafts(cx),
            CloseAnswer::DiscardDrafts => self.confirm_scene(review, cx),
            CloseAnswer::SaveScene => self.save_and_close(review, cx),
            CloseAnswer::DiscardScene => self.finish(review, cx),
            CloseAnswer::CloseWithoutLayout => self.shutdown(cx),
        }
    }

    fn drafts(&self, cx: &App) -> InspectorDrafts {
        self.inspector
            .read_with(cx, InspectorPanel::drafts)
            .unwrap_or_default()
    }

    fn validate(&self, review: &CloseReview, workspace: &Workspace, cx: &App) -> Result<()> {
        anyhow::ensure!(
            workspace.revision == review.revision
                && workspace.session.document().scene().id == review.document
                && self.drafts(cx) == review.drafts,
            "The scene or Inspector drafts changed during close. Review the changes and close again."
        );
        Ok(())
    }

    fn review_drafts(self, cx: &mut App) {
        self.state.set(RootCloseState::Open);
        if let Some(item) = self.dock.item_id_for_external_key("inspector")
            && let Err(error) = self.dock.update(
                |frame| {
                    frame.select_item_current(item)?;
                    Ok(())
                },
                cx,
            )
        {
            warn(&self.workspace, format!("Open Inspector: {error:#}"), cx);
        }
    }

    fn save_and_close(self, review: Rc<CloseReview>, cx: &mut App) {
        let workspace = self.workspace.clone();
        let state = self.state.clone();
        let close = self.clone();
        let reviewed = review.clone();
        if workspace
            .update(cx, |workspace, cx| {
                workspace.save_with_completion(
                    cx,
                    move |workspace, cx| close.validate(&reviewed, workspace, cx),
                    move |saved, cx| cx.defer(move |cx| self.after_save(saved, review, cx)),
                );
            })
            .is_err()
        {
            state.set(RootCloseState::Open);
        }
    }

    fn after_save(self, saved: bool, review: Rc<CloseReview>, cx: &mut App) {
        if saved {
            let result = self
                .workspace
                .read_with(cx, |workspace, cx| {
                    self.validate(&review, workspace, cx)?;
                    anyhow::ensure!(
                        !workspace.session.is_dirty(),
                        "The scene changed during save. Review the changes and close again."
                    );
                    Ok(())
                })
                .and_then(|result| result);
            match result {
                Ok(()) => {
                    self.finish(review, cx);
                    return;
                }
                Err(error) => warn(&self.workspace, error.to_string(), cx),
            }
        }
        self.state.set(RootCloseState::Open);
    }

    fn finish(self, review: Rc<CloseReview>, cx: &mut App) {
        if let Some(path) = &self.layout_path
            && let Err(error) = save_layout(&self.dock, path, cx)
        {
            warn_layout(&self.workspace, format!("Layout save: {error:#}"), cx);
            self.prompt(
                review,
                "Could not save the editor layout",
                &format!(
                    "{error:#}\n\nKeep the editor open to resolve the problem, or close and lose changes to the pane layout."
                ),
                &[
                    (CloseAnswer::Cancel, "Keep Editor Open"),
                    (CloseAnswer::CloseWithoutLayout, "Close Without Saving Layout"),
                ],
                cx,
            );
            return;
        }
        self.shutdown(cx);
    }

    fn shutdown(self, cx: &mut App) {
        self.state.set(RootCloseState::Closing);
        let job = self
            .workspace
            .update(cx, |workspace, cx| workspace.take_job(cx))
            .ok()
            .flatten();
        if let Some(job) = job {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .spawn(async move { drop(job) })
                    .await;
                cx.update(|cx| self.begin_native_shutdown(cx));
            })
            .detach();
            return;
        }
        self.begin_native_shutdown(cx);
    }

    fn begin_native_shutdown(self, cx: &mut App) {
        if let Err(error) = self.dock.begin_native_shutdown(cx) {
            self.state.set(RootCloseState::Open);
            warn(&self.workspace, format!("Editor close: {error:#}"), cx);
        }
    }
}

fn warn(workspace: &WeakEntity<Workspace>, message: impl Into<String>, cx: &mut App) {
    let _ = workspace.update(cx, |workspace, cx| workspace.warn(message, cx));
}
