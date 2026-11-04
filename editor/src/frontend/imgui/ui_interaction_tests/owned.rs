use std::sync::Arc;

use nara_ui_kernel::{
    UiActivationSource, UiFrame, UiFrameJournal, UiInputEvent, UiInteractionSession, UiRouteStamp,
    UiSeatId, UiSemanticRole, UiSurfaceId,
};

use crate::{
    EditorApplication, frontend::imgui::ImguiEditorFrontend, workspace::EditorPanelInstanceId,
};

pub(in crate::frontend::imgui) struct CpuOwnedInput {
    journal: UiFrameJournal,
    authority: nara_ui_kernel::__private::UiFrameJournalAuthority,
    session: UiInteractionSession,
    surface: Option<UiSurfaceId>,
    epoch: u64,
}

impl Default for CpuOwnedInput {
    fn default() -> Self {
        let (journal, authority) =
            nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
        Self {
            journal,
            authority,
            session: UiInteractionSession::default(),
            surface: None,
            epoch: 0,
        }
    }
}

pub(in crate::frontend::imgui) fn hierarchy_panel(
    application: &EditorApplication,
) -> EditorPanelInstanceId {
    application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == "nara.hierarchy")
        .expect("the UI fixture has a Hierarchy panel")
        .id()
}

pub(in crate::frontend::imgui) fn hierarchy_frame(
    frontend: &ImguiEditorFrontend,
    application: &EditorApplication,
) -> Arc<UiFrame> {
    Arc::clone(
        frontend.presentation.widgets[&hierarchy_panel(application)]
            .owned_hierarchy
            .as_ref()
            .expect("the UI Hierarchy paints Nara controls")
            .compiled_frame()
            .expect("the UI Hierarchy has a compiled CPU frame"),
    )
}

pub(in crate::frontend::imgui) fn button_visible(frame: &UiFrame, name: &str) -> bool {
    let Some(button) = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Button && node.name() == name)
    else {
        return false;
    };
    frame.layouts().iter().any(|layout| {
        layout.key() == Some(button.key())
            && layout.visible()
            && layout
                .clip_rect()
                .is_none_or(|clip| layout.rect().intersect(clip).is_some())
    })
}

impl CpuOwnedInput {
    pub(in crate::frontend::imgui) fn click(
        &mut self,
        frontend: &mut ImguiEditorFrontend,
        application: &EditorApplication,
        frame: &Arc<UiFrame>,
        name: &str,
    ) -> bool {
        let mut buttons = frame
            .semantics()
            .iter()
            .filter(|node| node.role() == UiSemanticRole::Button && node.name() == name);
        let button = buttons
            .next()
            .unwrap_or_else(|| panic!("the compiled UI Hierarchy has no Button {name:?}"));
        assert!(
            buttons.next().is_none(),
            "ambiguous Hierarchy Button {name:?}"
        );
        let focused = self.route(
            frontend,
            application,
            frame,
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: button.key().clone(),
            },
        );
        assert_eq!(focused, button.enabled(), "Hierarchy Button {name:?}");
        if focused {
            assert!(self.route(
                frontend,
                application,
                frame,
                UiInputEvent::ActivateFocused {
                    seat: UiSeatId::PRIMARY,
                    source: UiActivationSource::Keyboard,
                },
            ));
        }
        focused
    }

    pub(in crate::frontend::imgui) fn route(
        &mut self,
        frontend: &mut ImguiEditorFrontend,
        application: &EditorApplication,
        frame: &Arc<UiFrame>,
        input: UiInputEvent,
    ) -> bool {
        let environment = frame.environment();
        let panel = frontend
            .presentation
            .widgets
            .iter()
            .find(|(_, widgets)| {
                widgets
                    .incarnation
                    .serial
                    .checked_add(1)
                    .and_then(UiSurfaceId::new)
                    == Some(environment.surface_id())
            })
            .map(|(panel, _)| *panel)
            .expect("the CPU frame belongs to a current panel incarnation");
        if self
            .surface
            .is_some_and(|surface| surface != environment.surface_id())
        {
            let previous = frontend
                .presentation
                .widgets
                .iter()
                .find(|(_, widgets)| {
                    widgets
                        .incarnation
                        .serial
                        .checked_add(1)
                        .and_then(UiSurfaceId::new)
                        == self.surface
                })
                .map(|(panel, _)| *panel);
            if let Some(previous) = previous {
                frontend.cancel_owned_panel_input(previous).unwrap();
            }
            *self = Self::default();
        }
        if let Some(surface) = self.surface {
            assert_eq!(surface, environment.surface_id());
        } else {
            self.journal
                .install_surface(
                    environment.surface_id(),
                    environment.slot_generation().get(),
                )
                .unwrap();
            self.surface = Some(environment.surface_id());
        }
        self.epoch += 1;
        let receipt = self
            .authority
            .submit(&mut self.journal, Arc::clone(frame), self.epoch)
            .unwrap();
        let route = UiRouteStamp::from_receipt(receipt);
        let model = application.current_frame().unwrap();
        let authoring = application.current_ui_authoring_frame();
        for attempt in 0..2 {
            let outcome = self
                .session
                .dispatch(&self.journal, route, input.clone())
                .unwrap();
            let Some(dispatch) = outcome.consumed() else {
                return false;
            };
            frontend
                .reduce_owned_input(
                    panel,
                    frame,
                    dispatch,
                    &model,
                    authoring.as_deref(),
                    &mut |_| panic!("this CPU fixture does not request clipboard writes"),
                )
                .unwrap();
            if !dispatch.is_reconciliation() {
                return true;
            }
            assert_eq!(attempt, 0, "reconciliation must settle before input");
        }
        unreachable!()
    }
}
