use nara_text::TextEdit;
use nara_ui_kernel::{
    UiActivationSource, UiFrame, UiFrameJournal, UiInputEvent, UiInteractionSession, UiKey,
    UiRouteStamp, UiSeatId, UiSemanticRole, UiSurfaceId,
};

use super::*;
use crate::frontend::imgui::owned_inspector::InspectorBinding;
use crate::workspace::EditorPanelInstanceId;

pub(super) use crate::frontend::imgui::owned_inspector::InspectorOperation;

pub(super) struct CpuOwnedInput {
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

impl Interaction {
    pub(super) fn hierarchy_panel(&self) -> EditorPanelInstanceId {
        self.application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == "nara.hierarchy")
            .unwrap()
            .id()
    }

    fn hierarchy_frame(&self) -> Arc<UiFrame> {
        Arc::clone(
            self.frontend.presentation.widgets[&self.hierarchy_panel()]
                .owned_hierarchy
                .as_ref()
                .expect("the source hierarchy paints Nara controls")
                .compiled_frame()
                .unwrap(),
        )
    }

    pub(super) fn queue_hierarchy_click(&mut self, name: &str) {
        self.actions.clear();
        let panel = self.hierarchy_panel();
        let find = |frame: &UiFrame| {
            frame
                .semantics()
                .iter()
                .find(|node| node.role() == UiSemanticRole::Button && node.name() == name)
                .map(|node| (node.key().clone(), node.enabled()))
        };
        let mut frame = self.hierarchy_frame();
        if find(&frame).is_none() {
            self.frontend
                .scroll_owned_panel(panel, &frame, -f32::MAX)
                .unwrap();
            self.render();
        }
        let (frame, button, enabled) = loop {
            frame = self.hierarchy_frame();
            if let Some((button, enabled)) = find(&frame) {
                break (frame, button, enabled);
            }
            let delta = 48.0;
            assert!(
                self.frontend
                    .scroll_owned_panel(panel, &frame, delta)
                    .unwrap(),
                "missing Hierarchy Button {name:?}"
            );
            self.render();
        };
        let focused = self.route_owned(
            &frame,
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: button,
            },
        );
        assert_eq!(focused, enabled, "Hierarchy Button {name:?}");
        if focused {
            assert!(self.route_owned(
                &frame,
                UiInputEvent::ActivateFocused {
                    seat: UiSeatId::PRIMARY,
                    source: UiActivationSource::Keyboard,
                }
            ));
        }
    }

    pub(super) fn click_hierarchy(&mut self, name: &str) {
        self.queue_hierarchy_click(name);
        self.render();
        self.render();
    }

    fn owned_panel(&self) -> EditorPanelInstanceId {
        self.application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == "nara.inspector")
            .unwrap()
            .id()
    }

    fn owned_frame(&self) -> Arc<UiFrame> {
        Arc::clone(
            self.frontend.presentation.widgets[&self.owned_panel()]
                .owned_inspector
                .as_ref()
                .expect("the default Document tab paints Nara controls")
                .compiled_frame()
                .unwrap(),
        )
    }

    pub(super) fn find_owned_control(
        &self,
        group: Option<&str>,
        role: UiSemanticRole,
        name: &str,
    ) -> Option<(Arc<UiFrame>, UiKey)> {
        let frame = self.owned_frame();
        let parent = match group {
            Some(group) => Some(
                frame
                    .semantics()
                    .iter()
                    .find(|node| node.role() == UiSemanticRole::Group && node.name() == group)?
                    .key(),
            ),
            None => None,
        };
        let node = frame.semantics().iter().find(|node| {
            node.role() == role
                && node.name() == name
                && (group.is_none() || node.parent() == parent)
        })?;
        Some((Arc::clone(&frame), node.key().clone()))
    }

    fn owned_control(
        &mut self,
        group: Option<&str>,
        role: UiSemanticRole,
        name: &str,
    ) -> (Arc<UiFrame>, UiKey) {
        if let Some(control) = self.find_owned_control(group, role, name) {
            return control;
        }
        self.scroll_owned(-f32::MAX);
        loop {
            if let Some(control) = self.find_owned_control(group, role, name) {
                return control;
            }
            let step = self.owned_frame().environment().logical_size().height() * 0.75;
            assert!(
                self.scroll_owned(step),
                "missing owned {role:?} {group:?}/{name}"
            );
        }
    }

    pub(super) fn scroll_owned(&mut self, delta: f32) -> bool {
        let frame = self.owned_frame();
        let changed = self
            .frontend
            .scroll_owned_panel(self.owned_panel(), &frame, delta)
            .unwrap();
        if changed {
            self.render();
        }
        changed
    }

    fn route_owned(&mut self, frame: &Arc<UiFrame>, input: UiInputEvent) -> bool {
        let environment = frame.environment();
        let panel = self
            .frontend
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
            .expect("the frame belongs to a current panel incarnation");
        if self
            .owned_input
            .surface
            .is_some_and(|surface| surface != environment.surface_id())
        {
            let previous = self
                .frontend
                .presentation
                .widgets
                .iter()
                .find(|(_, widgets)| {
                    widgets
                        .incarnation
                        .serial
                        .checked_add(1)
                        .and_then(UiSurfaceId::new)
                        == self.owned_input.surface
                })
                .map(|(panel, _)| *panel);
            if let Some(previous) = previous {
                self.frontend.cancel_owned_panel_input(previous).unwrap();
            }
            self.owned_input = CpuOwnedInput::default();
        }
        let adapter = &mut self.owned_input;
        if let Some(surface) = adapter.surface {
            assert_eq!(surface, environment.surface_id());
        } else {
            adapter
                .journal
                .install_surface(
                    environment.surface_id(),
                    environment.slot_generation().get(),
                )
                .unwrap();
            adapter.surface = Some(environment.surface_id());
        }
        adapter.epoch += 1;
        let receipt = adapter
            .authority
            .submit(&mut adapter.journal, Arc::clone(frame), adapter.epoch)
            .unwrap();
        let route = UiRouteStamp::from_receipt(receipt);
        let model = self.frame();
        for attempt in 0..2 {
            let outcome = self
                .owned_input
                .session
                .dispatch(&self.owned_input.journal, route, input.clone())
                .unwrap();
            let Some(dispatch) = outcome.consumed() else {
                return false;
            };
            self.frontend
                .reduce_owned_input(panel, frame, dispatch, &model, &mut |_| {
                    panic!("this fixture does not request clipboard writes")
                })
                .unwrap();
            if !dispatch.is_reconciliation() {
                return true;
            }
            assert_eq!(attempt, 0, "reconciliation must settle before input");
        }
        unreachable!()
    }

    pub(super) fn queue_owned_click(&mut self, group: Option<&str>, name: &str) {
        self.actions.clear();
        let (frame, button) = self.owned_control(group, UiSemanticRole::Button, name);
        let enabled = frame
            .semantics()
            .iter()
            .find(|node| node.key() == &button)
            .unwrap()
            .enabled();
        let focused = self.route_owned(
            &frame,
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: button,
            },
        );
        assert_eq!(focused, enabled, "{group:?}/{name}");
        if focused {
            assert!(self.route_owned(
                &frame,
                UiInputEvent::ActivateFocused {
                    seat: UiSeatId::PRIMARY,
                    source: UiActivationSource::Keyboard,
                },
            ));
        }
    }

    pub(super) fn click_owned(&mut self, group: Option<&str>, name: &str) {
        self.queue_owned_click(group, name);
        self.render();
        self.render();
    }

    pub(super) fn try_replace_owned_text(
        &mut self,
        group: &str,
        name: &str,
        text: &str,
    ) -> Result<(), String> {
        let (frame, input) = self.owned_control(Some(group), UiSemanticRole::TextInput, name);
        assert!(self.route_owned(
            &frame,
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: input.clone(),
            },
        ));
        let panel = self.owned_panel();
        let token = self
            .frontend
            .owned_field_token(panel, &frame, &input)
            .unwrap();
        self.frontend
            .owned_edit(panel, &frame, &input, token, &TextEdit::SelectAll)?;
        let result =
            self.frontend
                .owned_edit(panel, &frame, &input, token, &TextEdit::Insert(text.into()));
        self.render();
        result
    }

    pub(super) fn replace_field(&mut self, field: &str, text: &str) {
        self.try_replace_owned_text(field, field, text).unwrap();
    }

    pub(super) fn queue_owned_operation(
        &mut self,
        path: &str,
        operation: InspectorOperation,
    ) -> Result<(), String> {
        self.actions.clear();
        let (frame, button) =
            self.owned_control(Some(path), UiSemanticRole::Button, operation.label());
        let panel = self.owned_panel();
        let InspectorBinding::Operation {
            field,
            operation: actual,
        } = self.frontend.presentation.widgets[&panel]
            .owned_inspector
            .as_ref()
            .unwrap()
            .binding(&frame, &button)
            .unwrap()
        else {
            panic!("the operation must retain its actual field owner")
        };
        assert_eq!(actual, operation);
        let model = self.frame();
        self.frontend
            .owned_operation(panel, &frame, &field, operation, &model)
    }

    pub(super) fn field_operation(
        &mut self,
        path: &str,
        operation: InspectorOperation,
    ) -> Result<(), String> {
        let result = self.queue_owned_operation(path, operation);
        self.render();
        self.render();
        result
    }
}
