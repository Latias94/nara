use std::{path::PathBuf, time::Duration};

use nara::{
    tooling::EditorActionBatch,
    tooling_egui::{
        EguiCentralViewportInput, EguiCentralViewportOutput, EguiEditorOutput, EguiEditorPanel,
        EguiEditorProjectAction, EguiEditorViewport, EguiEditorViewportIntent,
        EguiProjectManagerInput, EguiProjectOpenDraft, EguiProjectOpeningInput,
        EguiProjectRecoveryDiagnostic, EguiProjectRecoveryInput, EguiProjectShellAction,
        EguiProjectShellInput, EguiProjectShellOutput, EguiProjectShellPanel,
        EguiUiAuthoringImages, EguiUiAuthoringInput, EguiUiAuthoringMutations,
    },
};

use crate::{
    EditorApplication, EditorApplicationUpdate, EditorProjectAction,
    app::{
        EditorApplicationDispatch, EditorApplicationSurface, EditorProjectShellAction,
        EditorProjectShellModel,
    },
};

pub(crate) struct EditorWorkspaceRenderOutput {
    editor: EguiEditorOutput,
    project_browser_local_changed: bool,
}

impl EditorWorkspaceRenderOutput {
    fn into_action_batch(self) -> EditorActionBatch {
        self.editor.into_parts().0
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        EditorActionBatch,
        Option<EditorProjectAction>,
        EguiCentralViewportOutput,
        Option<EguiEditorViewportIntent>,
        bool,
    ) {
        let (actions, project_action, viewport, viewport_intent) = self.editor.into_parts();
        (
            actions,
            project_action.map(lower_project_action),
            viewport,
            viewport_intent,
            self.project_browser_local_changed,
        )
    }
}

/// Replaceable widget state for one private Editor application's egui presentation.
///
/// Recreating this value resets form/focus buffers, without creating or replacing a project
/// session, document, history or persistence owner.
#[derive(Debug, Default)]
pub struct EguiEditorFrontend {
    shell_panel: EguiProjectShellPanel,
    manager_entry: Option<u64>,
    manager_draft: EguiProjectOpenDraft,
    workspace_generation: Option<u64>,
    workspace_panel: EguiEditorPanel,
}

impl EguiEditorFrontend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Submits a widget batch and observes its exact result before advancing the application.
    pub fn submit_and_drive(
        &mut self,
        application: &mut EditorApplication,
        batch: EditorActionBatch,
        real_delta: Duration,
    ) -> EditorApplicationUpdate {
        let dispatch = application.dispatch_editor_actions(batch);
        self.observe_dispatch(&dispatch);
        application.drive_editor_frame(real_delta, dispatch.actions_observed(), true)
    }

    pub(crate) fn observe_dispatch(&mut self, dispatch: &EditorApplicationDispatch) {
        if let Some(feedback) = dispatch.feedback() {
            match feedback {
                Ok(report) => self.workspace_panel.observe_action_report(report),
                Err(rejection) => self.workspace_panel.observe_action_rejection(rejection),
            }
        }
    }

    /// Draws the active workspace and returns its existing semantic action batch.
    #[must_use]
    pub fn show(
        &mut self,
        root: &mut egui::Ui,
        application: &mut EditorApplication,
    ) -> Option<EditorActionBatch> {
        self.show_with_viewport(
            root,
            application,
            EguiCentralViewportInput::unavailable(),
            EguiEditorViewport::Authoring,
        )
        .map(EditorWorkspaceRenderOutput::into_action_batch)
    }

    #[must_use]
    pub(crate) fn show_with_viewport(
        &mut self,
        root: &mut egui::Ui,
        application: &mut EditorApplication,
        viewport: EguiCentralViewportInput,
        selected_viewport: EguiEditorViewport,
    ) -> Option<EditorWorkspaceRenderOutput> {
        self.synchronize_workspace(application);
        let EditorApplicationSurface::Workspace {
            frame,
            ui_authoring,
            read_only,
        } = application.surface()
        else {
            return None;
        };
        let mut editor_output = egui::Frame::central_panel(root.style())
            .show(root, |ui| {
                let field_editing_enabled = |key: &nara::ui::__private::UiAuthoringFieldKey| {
                    ui_authoring
                        .as_ref()
                        .is_some_and(|authoring| authoring.field_editing_enabled(key))
                };
                let mut lower_continuous_field =
                    |key: &nara::ui::__private::UiAuthoringFieldKey,
                     value: nara::ui::__private::UiAuthoringFieldValue| {
                        ui_authoring
                            .as_ref()
                            .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                            .lower_field_edit(key, value)
                            .map_err(|error| error.to_string())
                    };
                let mut lower_discrete_field =
                    |key: &nara::ui::__private::UiAuthoringFieldKey,
                     value: nara::ui::__private::UiAuthoringFieldValue| {
                        ui_authoring
                            .as_ref()
                            .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                            .lower_discrete_field_edit(key, value)
                            .map_err(|error| error.to_string())
                    };
                let mut lower_direct = |edit: &nara::ui::__private::UiAuthoringDirectEdit| {
                    ui_authoring
                        .as_ref()
                        .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                        .lower_direct_edit(edit)
                        .map_err(|error| error.to_string())
                };
                let mut lower_structure = |intent: &nara::ui::__private::UiStructureIntent| {
                    ui_authoring
                        .as_ref()
                        .ok_or_else(|| "UI authoring is unavailable".to_owned())?
                        .lower_structure_intent(intent)
                        .map_err(|error| error.to_string())
                };
                let authoring = ui_authoring.as_ref().and_then(|authoring| {
                    (authoring.editor_frame() == frame.revision()).then(|| {
                        EguiUiAuthoringInput::new(
                            authoring.document(),
                            authoring.revision(),
                            authoring.selection(),
                            authoring.gesture(),
                            authoring.projection(),
                            EguiUiAuthoringImages::new(
                                authoring.image_choice_generation(),
                                authoring.image_choices(),
                            ),
                            EguiUiAuthoringMutations::new(
                                &field_editing_enabled,
                                authoring.direct_editing_enabled(),
                                authoring.structure_editing_enabled(),
                                &mut lower_continuous_field,
                                &mut lower_discrete_field,
                                &mut lower_direct,
                                &mut lower_structure,
                            ),
                        )
                    })
                });
                self.workspace_panel.show_with_authoring_lock(
                    ui,
                    &frame,
                    authoring,
                    viewport,
                    selected_viewport,
                    read_only,
                )
            })
            .inner;
        let project_browser_local_changed = editor_output
            .take_project_browser_local_actions()
            .into_iter()
            .fold(false, |changed, action| {
                application.apply_project_browser_local_action(action) || changed
            });
        let replacement_opening = application.project_shell_model().filter(|model| {
            matches!(
                model,
                EditorProjectShellModel::Opening {
                    replacement: true,
                    ..
                }
            )
        });
        let mut shell_output = None;
        if let Some(model) = replacement_opening {
            egui::Window::new("Project transition")
                .id(egui::Id::new("nara_editor_replacement_progress"))
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(root.ctx(), |ui| {
                    shell_output = Some(self.show_project_shell_contents(ui, model));
                });
        }
        if let Some(output) = shell_output {
            let _ = self.consume_shell_output(application, output);
        }
        Some(EditorWorkspaceRenderOutput {
            editor: editor_output,
            project_browser_local_changed,
        })
    }

    fn synchronize_workspace(&mut self, application: &mut EditorApplication) {
        let generation = application.workspace_generation();
        if self.workspace_generation != generation {
            self.workspace_panel = EguiEditorPanel::new();
            self.workspace_generation = generation;
        }
    }

    pub(crate) fn select_project_browser_sidebar(&mut self) -> bool {
        self.workspace_generation.is_some() && self.workspace_panel.select_project_browser_sidebar()
    }

    pub(crate) fn project_browser_sidebar_selected(&self) -> bool {
        self.workspace_generation.is_some()
            && self.workspace_panel.project_browser_sidebar_selected()
    }

    pub(crate) fn show_project_shell(
        &mut self,
        root: &mut egui::Ui,
        application: &mut EditorApplication,
    ) -> EditorApplicationUpdate {
        self.synchronize_workspace(application);
        let Some(model) = application.project_shell_model() else {
            return EditorApplicationUpdate::idle();
        };
        let output = egui::Frame::central_panel(root.style())
            .show(root, |ui| self.show_project_shell_contents(ui, model))
            .inner;
        self.consume_shell_output(application, output)
    }

    fn show_project_shell_contents(
        &mut self,
        ui: &mut egui::Ui,
        model: EditorProjectShellModel<'_>,
    ) -> EguiProjectShellOutput {
        match model {
            EditorProjectShellModel::ProjectManager {
                entry_generation,
                suggested_request,
            } => {
                if self.manager_entry != Some(entry_generation) {
                    self.manager_draft = EguiProjectOpenDraft::new(
                        suggested_request.project_root().to_string_lossy(),
                        suggested_request.owned_profile().unwrap_or_default(),
                    );
                    self.manager_entry = Some(entry_generation);
                }
                self.shell_panel.show(
                    ui,
                    EguiProjectShellInput::ProjectManager(EguiProjectManagerInput::new(
                        entry_generation,
                        &self.manager_draft,
                    )),
                )
            }
            EditorProjectShellModel::Opening {
                entry_generation,
                attempt_generation,
                request,
                phase,
                summary,
                replacement,
            } => {
                let draft = EguiProjectOpenDraft::new(
                    request.project_root().to_string_lossy(),
                    request.owned_profile().unwrap_or_default(),
                );
                let input = EguiProjectOpeningInput::new(
                    entry_generation,
                    &draft,
                    attempt_generation,
                    phase,
                    summary,
                );
                self.shell_panel.show(
                    ui,
                    if replacement {
                        EguiProjectShellInput::OpeningReplacement(input)
                    } else {
                        EguiProjectShellInput::OpeningInitial(input)
                    },
                )
            }
            EditorProjectShellModel::Recovery {
                entry_generation,
                attempt_generation,
                request,
                diagnostics,
            } => {
                let draft = EguiProjectOpenDraft::new(
                    request.project_root().to_string_lossy(),
                    request.owned_profile().unwrap_or_default(),
                );
                let diagnostics = diagnostics
                    .iter()
                    .map(|diagnostic| {
                        EguiProjectRecoveryDiagnostic::new(
                            &diagnostic.code,
                            &diagnostic.phase,
                            diagnostic.relative_source.as_deref(),
                            &diagnostic.summary,
                            &diagnostic.impact,
                            &diagnostic.repair_hint,
                        )
                    })
                    .collect::<Vec<_>>();
                self.shell_panel.show(
                    ui,
                    EguiProjectShellInput::Recovery(EguiProjectRecoveryInput::new(
                        entry_generation,
                        &draft,
                        attempt_generation,
                        &diagnostics,
                    )),
                )
            }
        }
    }

    fn consume_shell_output(
        &mut self,
        application: &mut EditorApplication,
        output: EguiProjectShellOutput,
    ) -> EditorApplicationUpdate {
        let (draft, action) = output.into_parts();
        self.manager_draft = draft;
        let Some(action) = action else {
            return EditorApplicationUpdate::idle();
        };
        let action = match action {
            EguiProjectShellAction::OpenProject => EditorProjectShellAction::OpenProject {
                project_root: PathBuf::from(self.manager_draft.project_path().trim()),
                profile: Some(self.manager_draft.profile().to_owned()),
            },
            EguiProjectShellAction::Retry => EditorProjectShellAction::Retry,
            EguiProjectShellAction::Cancel => EditorProjectShellAction::Cancel,
            EguiProjectShellAction::CloseProject => EditorProjectShellAction::CloseProject,
            EguiProjectShellAction::ReturnToProjectManager => {
                EditorProjectShellAction::ReturnToProjectManager
            }
            EguiProjectShellAction::Exit => EditorProjectShellAction::Exit,
        };
        application.submit_shell_action(action)
    }
}

fn lower_project_action(action: EguiEditorProjectAction) -> EditorProjectAction {
    match action {
        EguiEditorProjectAction::OpenProject(draft) => EditorProjectAction::OpenProject {
            project_root: PathBuf::from(draft.project_path().trim()),
            profile: Some(draft.profile().to_owned()),
        },
        EguiEditorProjectAction::CloseProject => EditorProjectAction::CloseProject,
    }
}
