use std::sync::Arc;

use nara::{project_host::EditorUiAuthoringFrame, tooling::EditorFrameModel};
use nara_text::FontFace;
use nara_ui_kernel::{UiDispatchBatch, UiEnvironment, UiFrame, UiKey, UiRootKey};

use crate::{EditorPanel, PanelActionDispatch, PanelSubmissionId};

use super::controls::HierarchyControls;

pub(crate) mod input;
mod shell;

pub(crate) struct DrawnPanel(PanelControls);

enum PanelControls {
    Shell(shell::ShellControls),
    Hierarchy {
        root: UiRootKey,
        fonts: Vec<FontFace>,
        owner: Option<HierarchyControls>,
    },
}

impl EditorPanel for DrawnPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.action_finished(id, &result),
            PanelControls::Hierarchy {
                owner: Some(owner), ..
            } => owner.action_finished(id, &result),
            PanelControls::Hierarchy { .. } => {}
        }
    }
}

impl DrawnPanel {
    pub(crate) fn shell(root: UiRootKey, fonts: Vec<FontFace>) -> Result<Self, String> {
        Ok(Self(PanelControls::Shell(shell::ShellControls::new(
            root, fonts,
        )?)))
    }

    pub(crate) fn hierarchy(root: UiRootKey, fonts: Vec<FontFace>) -> Self {
        Self(PanelControls::Hierarchy {
            root,
            fonts,
            owner: None,
        })
    }

    pub(crate) fn compile(
        &mut self,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        environment: UiEnvironment,
    ) -> Result<Arc<UiFrame>, String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.compile(model, environment),
            PanelControls::Hierarchy { root, fonts, owner } => {
                let specialized = ui.is_some();
                if owner
                    .as_ref()
                    .is_some_and(|owner| owner.is_ui() != specialized)
                {
                    if let Some(owner) = owner.as_mut() {
                        owner.cancel_input()?;
                    }
                    *owner = None;
                }
                if owner.is_none() {
                    *owner = Some(HierarchyControls::new(
                        root.clone(),
                        fonts.clone(),
                        specialized,
                    )?);
                }
                owner
                    .as_mut()
                    .expect("the current projection was constructed")
                    .compile(model, ui, environment, Default::default(), false)
            }
        }
    }

    pub(crate) fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        match &self.0 {
            PanelControls::Shell(shell) => shell.retains_frame(frame),
            PanelControls::Hierarchy { owner, .. } => owner
                .as_ref()
                .is_some_and(|owner| owner.retains_frame(frame)),
        }
    }

    pub(crate) fn cancel_input(&mut self) -> Result<(), String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.cancel_input(),
            PanelControls::Hierarchy {
                owner: Some(owner), ..
            } => owner.cancel_input(),
            PanelControls::Hierarchy { .. } => Ok(()),
        }
    }

    pub(crate) fn reduce(
        &mut self,
        frame: &Arc<UiFrame>,
        batch: &UiDispatchBatch,
        context: &mut crate::EditorPanelContext<'_, '_>,
        ui: Option<&EditorUiAuthoringFrame>,
    ) -> Result<(), String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => {
                let intents = shell.dispatch(frame, batch)?;
                for intent in intents.intents() {
                    let action =
                        match shell.activate(frame, intent.target(), context.model().frame()) {
                            Ok(action) => action,
                            Err(_) => continue,
                        };
                    if let Some(action) = action {
                        context.submit(action).map_err(|error| error.to_string())?;
                    }
                }
            }
            PanelControls::Hierarchy {
                owner: Some(owner), ..
            } => {
                let intents = owner.dispatch(frame, batch)?;
                for intent in intents.intents() {
                    let submission = match owner.activate(
                        frame,
                        intent.target(),
                        context.model().frame(),
                        ui,
                        false,
                    ) {
                        Ok(submission) => submission,
                        Err(error) => {
                            owner.submission_failed(error);
                            continue;
                        }
                    };
                    if let Some(submission) = submission {
                        let action =
                            match submission.into_action(context.model().frame(), ui, false) {
                                Ok(action) => action,
                                Err(error) => {
                                    owner.submission_failed(error);
                                    continue;
                                }
                            };
                        owner.admitted(context.submit(action));
                    }
                }
            }
            PanelControls::Hierarchy { .. } => return Err("Hierarchy controls are retired".into()),
        }
        Ok(())
    }

    fn hierarchy_mut(&mut self) -> Option<&mut HierarchyControls> {
        match &mut self.0 {
            PanelControls::Hierarchy { owner, .. } => owner.as_mut(),
            PanelControls::Shell(_) => None,
        }
    }

    pub(crate) fn projection_changes(&self, ui: bool) -> bool {
        matches!(&self.0, PanelControls::Hierarchy { owner: Some(owner), .. } if owner.is_ui() != ui)
    }

    pub(crate) fn invalidated_controls(
        &self,
        previous: &Arc<UiFrame>,
        candidate: &Arc<UiFrame>,
    ) -> Result<Vec<UiKey>, String> {
        match &self.0 {
            PanelControls::Hierarchy {
                owner: Some(owner), ..
            } => owner.invalidated_controls(previous, candidate),
            PanelControls::Shell(_) => Ok(Vec::new()),
            _ => Err("Hierarchy controls are retired".into()),
        }
    }
}
