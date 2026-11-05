use std::sync::Arc;

use nara::{project_host::EditorUiAuthoringFrame, tooling::EditorFrameModel};
use nara_text::FontFace;
use nara_ui_kernel::{UiActionId, UiDispatchBatch, UiEnvironment, UiFrame, UiKey, UiRootKey};
use nara_ui_widgets::UiAppearance;

use crate::{EditorPanel, PanelActionDispatch, PanelSubmissionId};

use super::controls::HierarchyControls;

pub(crate) mod input;
mod inspector;
mod shell;
mod text_controls;

pub(crate) struct DrawnPanel(PanelControls);

enum PanelControls {
    Shell(shell::ShellControls),
    Inspector(inspector::InspectorControls),
    Hierarchy {
        root: UiRootKey,
        fonts: Vec<FontFace>,
        owner: Option<HierarchyControls>,
        appearance: UiAppearance,
    },
}

impl EditorPanel for DrawnPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.action_finished(id, &result),
            PanelControls::Inspector(inspector) => inspector.action_finished(id, &result),
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
            root,
            UiRootKey::runtime("nara.drawn", "workspace-menu")
                .map_err(|error| error.to_string())?,
            fonts,
        )?)))
    }

    pub(crate) fn hierarchy(root: UiRootKey, fonts: Vec<FontFace>) -> Self {
        Self(PanelControls::Hierarchy {
            root,
            fonts,
            owner: None,
            appearance: UiAppearance::default(),
        })
    }

    pub(crate) fn inspector(root: UiRootKey, fonts: Vec<FontFace>) -> Result<Self, String> {
        Ok(Self(PanelControls::Inspector(
            inspector::InspectorControls::new(root, fonts)?,
        )))
    }

    pub(crate) fn set_appearance(&mut self, appearance: UiAppearance) -> Result<(), String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.set_appearance(appearance)?,
            PanelControls::Inspector(inspector) => inspector.set_appearance(appearance),
            PanelControls::Hierarchy {
                appearance: current,
                owner,
                ..
            } => {
                *current = appearance;
                if let Some(owner) = owner {
                    owner.set_appearance(appearance);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn compile_panel(
        &mut self,
        context: &mut crate::EditorPanelContext<'_, '_>,
        ui: Option<&EditorUiAuthoringFrame>,
        environment: UiEnvironment,
    ) -> Result<Arc<UiFrame>, String> {
        if let PanelControls::Inspector(inspector) = &mut self.0 {
            inspector.compile(context, environment)
        } else {
            self.compile(context.model().frame(), ui, environment)
        }
    }

    pub(crate) fn compile(
        &mut self,
        model: &EditorFrameModel,
        ui: Option<&EditorUiAuthoringFrame>,
        environment: UiEnvironment,
    ) -> Result<Arc<UiFrame>, String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.compile(model, environment),
            PanelControls::Inspector(_) => {
                Err("Inspector compilation requires scoped document access".into())
            }
            PanelControls::Hierarchy {
                root,
                fonts,
                owner,
                appearance,
            } => {
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
                let theme = nara_ui_kernel::UiTheme::new(
                    environment.theme_revision().get(),
                    appearance.palette().kernel_palette(),
                )
                .map_err(|error| error.to_string())?;
                owner
                    .as_mut()
                    .expect("the current projection was constructed")
                    .set_appearance(*appearance);
                owner
                    .as_mut()
                    .expect("the current projection was constructed")
                    .compile(model, ui, environment, theme, false)
            }
        }
    }

    pub(crate) fn retains_frame(&self, frame: &Arc<UiFrame>) -> bool {
        match &self.0 {
            PanelControls::Shell(shell) => shell.retains_frame(frame),
            PanelControls::Inspector(inspector) => inspector.retains_frame(frame),
            PanelControls::Hierarchy { owner, .. } => owner
                .as_ref()
                .is_some_and(|owner| owner.retains_frame(frame)),
        }
    }

    fn owns_action(&self, frame: &Arc<UiFrame>, key: &UiKey, action: &UiActionId) -> bool {
        match &self.0 {
            PanelControls::Shell(shell) => shell.owns_action(frame, key, action),
            PanelControls::Inspector(inspector) => inspector.owns_action(frame, key, action),
            PanelControls::Hierarchy { owner, .. } => owner
                .as_ref()
                .is_some_and(|owner| owner.owns_action(frame, key, action)),
        }
    }

    fn input_basis_is_current(&self, frame: &Arc<UiFrame>) -> bool {
        match &self.0 {
            PanelControls::Shell(shell) => shell.input_basis_is_current(frame),
            _ => self.retains_frame(frame),
        }
    }

    pub(crate) fn cancel_input(&mut self) -> Result<(), String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.cancel_input(),
            PanelControls::Inspector(inspector) => inspector.cancel_input(),
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
            PanelControls::Inspector(inspector) => inspector.reduce(frame, batch, context)?,
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

    fn text_controls(&mut self) -> Option<text_controls::TextControls<'_>> {
        match &mut self.0 {
            PanelControls::Hierarchy { owner, .. } => {
                owner.as_mut().map(text_controls::TextControls::Hierarchy)
            }
            PanelControls::Inspector(inspector) => {
                Some(text_controls::TextControls::Inspector(inspector))
            }
            PanelControls::Shell(_) => None,
        }
    }

    fn scroll(&mut self, frame: &Arc<UiFrame>, delta: f32) -> Result<bool, String> {
        match &mut self.0 {
            PanelControls::Hierarchy {
                owner: Some(owner), ..
            } => owner.scroll(frame, delta),
            PanelControls::Inspector(inspector) => inspector.scroll(frame, delta),
            PanelControls::Shell(shell) => shell.scroll(frame, delta),
            _ => Ok(false),
        }
    }

    fn flush_services(
        &mut self,
        services: &mut crate::native::EditorPanelServices<'_>,
    ) -> Result<(), String> {
        if let PanelControls::Inspector(inspector) = &mut self.0 {
            inspector.flush_services(services)?;
        }
        Ok(())
    }

    fn dismiss_menu(&mut self) -> Result<bool, String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.dismiss_menu(),
            _ => Ok(false),
        }
    }

    fn menu_outside(&self, frame: &Arc<UiFrame>, point: nara_ui_kernel::UiPoint) -> bool {
        match &self.0 {
            PanelControls::Shell(shell) => shell.menu_outside(frame, point),
            _ => false,
        }
    }

    fn take_focus_request(&mut self, frame: &Arc<UiFrame>) -> Option<UiKey> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.take_focus_request(frame),
            _ => None,
        }
    }

    fn navigate_menu(&mut self, frame: &Arc<UiFrame>, previous: bool) -> Result<bool, String> {
        match &mut self.0 {
            PanelControls::Shell(shell) => shell.navigate_menu(frame, previous),
            _ => Ok(false),
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
            PanelControls::Inspector(inspector) => {
                inspector.invalidated_controls(previous, candidate)
            }
            _ => Err("Hierarchy controls are retired".into()),
        }
    }
}
