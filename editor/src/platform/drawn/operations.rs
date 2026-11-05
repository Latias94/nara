use nara_ui_kernel::{UiAccessibilityInput, UiFrame, UiSemanticSnapshot};

use crate::{
    PanelSubmissionId,
    native::{EditorPanelImageObservation, EditorPanelInputKind, EditorPanelPlatformEvent},
    workspace::EditorPanelInstanceId,
};

use super::DrawnNative;

pub(super) struct Observation {
    pub(super) image: EditorPanelImageObservation<UiFrame>,
    pub(super) semantics: UiSemanticSnapshot,
}

impl DrawnNative {
    pub(super) fn observe(&mut self) -> Result<Vec<Observation>, String> {
        self.native
            .observe(&self.application)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|image| {
                let token = image.endpoint().token().clone();
                let surface = self
                    .surfaces
                    .get(&token.instance())
                    .ok_or("The observed panel Surface is retired")?;
                let journal = self
                    .graphics
                    .as_ref()
                    .and_then(|graphics| graphics.journal(token.instance()))
                    .ok_or("The observed panel presenter is retired")?;
                let semantics = self
                    .panels
                    .inspect(&self.application, &token, |panel, _| {
                        surface.input.observe(panel, journal, image.snapshot())
                    })
                    .ok_or("The observed panel state is retired")??;
                Ok(Observation { image, semantics })
            })
            .collect()
    }

    pub(super) fn operate(
        &mut self,
        observation: &Observation,
        request: &UiAccessibilityInput,
    ) -> Result<Vec<(EditorPanelInstanceId, PanelSubmissionId)>, String> {
        let ui = self.application.current_ui_authoring_frame();
        let surfaces = &mut self.surfaces;
        let graphics = self.graphics.as_ref();
        let mut error = None;
        let outcome = self
            .native
            .invoke_observed(
                &self.application,
                &mut self.panels,
                &observation.image,
                |panel, context, event, services| {
                    if error.is_some()
                        && matches!(event.kind, EditorPanelInputKind::ObservedOperation)
                    {
                        return;
                    }
                    let instance = event.endpoint.instance();
                    let Some(surface) = surfaces.get_mut(&instance) else {
                        error.get_or_insert_with(|| "The observed Surface is retired".to_owned());
                        return;
                    };
                    let Some(journal) = graphics.and_then(|graphics| graphics.journal(instance))
                    else {
                        error.get_or_insert_with(|| "The observed presenter is retired".to_owned());
                        return;
                    };
                    let result = if matches!(event.kind, EditorPanelInputKind::ObservedOperation) {
                        surface.input.operate(
                            panel,
                            context,
                            event,
                            services,
                            journal,
                            ui.as_deref(),
                            request,
                        )
                    } else {
                        surface.input.deliver(
                            panel,
                            context,
                            event,
                            services,
                            journal,
                            ui.as_deref(),
                        )
                    };
                    if let Err(failure) = result {
                        error.get_or_insert(failure);
                    }
                },
            )
            .map_err(|error| error.to_string())?;
        if let Some(error) = error {
            return Err(error);
        }
        let ids = outcome
            .submission
            .as_ref()
            .map(|candidate| candidate.submissions().collect())
            .unwrap_or_default();
        if let Some(candidate) = outcome.submission {
            self.panels.submit_and_drive(
                &mut self.application,
                candidate,
                std::time::Duration::ZERO,
            );
        }
        Ok(ids)
    }

    pub(super) fn observed_input(
        &mut self,
        observation: &Observation,
        event: EditorPanelPlatformEvent,
    ) -> Result<bool, String> {
        self.dispatch_input(Some(event), Some(&observation.image))
    }
}
