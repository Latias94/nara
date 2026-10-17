use std::collections::{BTreeMap, BTreeSet};

use dear_imgui_rs::Ui;
use nara::tooling::{
    EditorAction, EditorActionBatchReport, EditorActionOutcome, EditorActionRejection,
    EditorDocumentId, EditorGestureAction, EditorGestureCancelReason, EditorGestureId,
    EditorGestureOutcome, EditorGestureView,
};

use crate::workspace::{
    EditorPanelInstanceId, EditorPanelModel, EditorPanelTarget, EditorPanelWorkspace,
};

use super::{ImguiFrontendOutput, ImguiPanelInput};

/// Optional painting companion for a trusted source-built panel contribution.
/// The companion observes projections and emits the same admitted application actions as built-ins.
pub type ImguiEditorPanelPainter = fn(&Ui, &mut ImguiEditorPanelContext<'_, '_>);

pub struct ImguiEditorPanelContext<'scope, 'frame> {
    pub(super) model: EditorPanelModel<'frame>,
    pub(super) widgets: &'scope mut ImguiPanelWidgets,
    pub(super) input: &'scope ImguiPanelInput<'frame>,
    pub(super) actions: &'scope mut Vec<EditorAction>,
    pub(super) output: &'scope mut ImguiFrontendOutput,
}

impl ImguiEditorPanelContext<'_, '_> {
    #[must_use]
    pub fn model(&self) -> EditorPanelModel<'_> {
        self.model
    }

    pub fn emit(&mut self, action: EditorAction) {
        self.actions.push(action);
    }
}

#[derive(Default)]
pub(super) struct ImguiPanelWidgets {
    pub document: Option<EditorDocumentId>,
    pub target: EditorPanelTarget,
    pub authoring: super::authoring::ImguiAuthoringPanels,
    pub project: super::project::ImguiProjectPanels,
    pub ui_authoring: super::ui_authoring::ImguiUiAuthoringPanels,
    pub scene_view: super::view::ImguiSceneView,
    pub game_view: super::game::ImguiGameView,
    pub project_inspector: bool,
}

struct PendingPanelGesture {
    instance: EditorPanelInstanceId,
    target: EditorPanelTarget,
    action_index: usize,
}

pub(super) struct ImguiPanelPresentation {
    pub painters: BTreeMap<String, ImguiEditorPanelPainter>,
    pub widgets: BTreeMap<EditorPanelInstanceId, ImguiPanelWidgets>,
    gesture_owner: Option<(EditorPanelInstanceId, EditorPanelTarget, EditorGestureId)>,
    gesture_owner_retired: bool,
    pending_gestures: Vec<PendingPanelGesture>,
    pub painted: BTreeSet<EditorPanelInstanceId>,
}

impl Default for ImguiPanelPresentation {
    fn default() -> Self {
        let builtins: [(&str, ImguiEditorPanelPainter); 6] = [
            ("nara.hierarchy", super::paint_hierarchy),
            ("nara.project-browser", super::paint_project_browser),
            ("nara.authoring-view", super::paint_authoring_view),
            ("nara.game-view", super::paint_game_view),
            ("nara.inspector", super::paint_inspector),
            ("nara.diagnostics", super::paint_diagnostics),
        ];
        Self {
            painters: builtins
                .into_iter()
                .map(|(id, painter)| (id.to_owned(), painter))
                .collect(),
            widgets: BTreeMap::new(),
            gesture_owner: None,
            gesture_owner_retired: false,
            pending_gestures: Vec::new(),
            painted: BTreeSet::new(),
        }
    }
}

impl ImguiPanelPresentation {
    pub fn reset_widgets(&mut self) {
        self.widgets.clear();
        self.gesture_owner_retired |= self.gesture_owner.is_some();
    }

    pub fn observe_panel_actions(
        &mut self,
        instance: EditorPanelInstanceId,
        target: &EditorPanelTarget,
        first_action: usize,
        actions: &[EditorAction],
    ) {
        for (action_index, action) in actions.iter().enumerate().skip(first_action) {
            if matches!(
                action,
                EditorAction::Gesture(EditorGestureAction::Begin { .. })
            ) {
                self.pending_gestures.push(PendingPanelGesture {
                    instance,
                    target: target.clone(),
                    action_index,
                });
            }
        }
    }

    pub fn reconcile_gesture(
        &mut self,
        workspace: &EditorPanelWorkspace,
        active: Option<&EditorGestureView>,
        actions: &mut Vec<EditorAction>,
    ) {
        let Some(active) = active else {
            self.gesture_owner = None;
            self.gesture_owner_retired = false;
            return;
        };
        let Some((owner, target, gesture)) = &self.gesture_owner else {
            return;
        };
        if *gesture != active.stamp().id() {
            self.gesture_owner = None;
            self.gesture_owner_retired = false;
            return;
        }
        let available = workspace.layout().instance(*owner).is_some_and(|instance| {
            instance.is_visible() && instance.target() == target && self.painted.contains(owner)
        });
        if !available || self.gesture_owner_retired {
            actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: *gesture,
                reason: EditorGestureCancelReason::TargetRetired,
            }));
            self.widgets.remove(owner);
            self.gesture_owner_retired = true;
        }
    }

    pub fn observe_report(&mut self, report: &EditorActionBatchReport) {
        for pending in self.pending_gestures.drain(..) {
            if let Some(EditorActionOutcome::Gesture(EditorGestureOutcome::Began(view))) =
                report.outcomes().get(pending.action_index)
            {
                self.gesture_owner = Some((pending.instance, pending.target, view.stamp().id()));
                self.gesture_owner_retired = false;
            } else if self
                .gesture_owner
                .as_ref()
                .is_none_or(|(owner, _, _)| *owner != pending.instance)
            {
                self.widgets.remove(&pending.instance);
            }
        }
        for widgets in self.widgets.values_mut() {
            widgets.ui_authoring.observe_report(report);
            widgets.project.observe_action_report(report);
        }
    }

    pub fn observe_rejection(&mut self, rejection: EditorActionRejection) {
        for pending in self.pending_gestures.drain(..) {
            if self
                .gesture_owner
                .as_ref()
                .is_none_or(|(owner, _, _)| *owner != pending.instance)
            {
                self.widgets.remove(&pending.instance);
            }
        }
        for widgets in self.widgets.values_mut() {
            widgets.ui_authoring.observe_rejection(rejection);
            widgets.project.observe_action_rejection(rejection);
        }
    }
}

pub(super) fn panel_window_name(title: &str, instance: EditorPanelInstanceId) -> String {
    format!("{title}###nara-panel-{}", instance.0)
}

pub(super) fn show_panel_instances(ui: &Ui, workspace: &mut EditorPanelWorkspace) -> bool {
    let instances = workspace.layout().instances().to_vec();
    let mut changed = false;
    for panel in instances {
        let title = workspace
            .registry()
            .descriptor(panel.kind())
            .map_or(panel.kind(), |descriptor| descriptor.title());
        let identity = format!("panel-instance:{}", panel.id().0);
        let _id = ui.push_id(identity.as_str());
        let mut visible = panel.is_visible();
        let clicked = ui.checkbox(format!("{title} #{}###visible", panel.id().0), &mut visible);
        #[cfg(test)]
        super::ui_interaction_tests::record_control(ui, identity);
        if clicked {
            if visible {
                workspace.layout_mut().reopen(panel.id());
            } else {
                workspace.layout_mut().set_visible(panel.id(), false);
            }
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_instances_keep_distinct_native_identity_across_title_changes() {
        let mut workspace = EditorPanelWorkspace::default();
        let first = workspace
            .open("nara.inspector", EditorPanelTarget::ActiveDocument)
            .unwrap();
        let second = workspace
            .open("nara.inspector", EditorPanelTarget::ActiveDocument)
            .unwrap();
        let first_name = panel_window_name("Inspector", first);
        let renamed = panel_window_name("Inspector (read only)", first);
        assert_ne!(first_name, panel_window_name("Inspector", second));
        assert_eq!(
            first_name.split_once("###").unwrap().1,
            renamed.split_once("###").unwrap().1
        );
        workspace.layout_mut().close(first);
        workspace.layout_mut().reopen(first);
        assert_eq!(first_name, panel_window_name("Inspector", first));
    }

    #[test]
    fn instance_checkbox_reopens_the_same_source_inspector_without_retargeting() {
        use dear_imgui_rs::{Condition, Context, MouseButton};

        use crate::workspace::EditorPanelSourceKind;

        use super::super::ui_interaction_tests::{clear_controls, control_rect};

        fn render(context: &mut Context, workspace: &mut EditorPanelWorkspace) -> bool {
            clear_controls();
            let ui = context.frame();
            let changed = ui
                .window("Workspace instances")
                .position([20.0, 20.0], Condition::Always)
                .size([400.0, 450.0], Condition::Always)
                .build(|| show_panel_instances(ui, workspace))
                .unwrap_or(false);
            let _ = context.render_legacy();
            changed
        }

        let mut workspace = EditorPanelWorkspace::default();
        let target = EditorPanelTarget::Source {
            kind: EditorPanelSourceKind::Prefab,
            locator: "ui/menu.prefab.json".to_owned(),
        };
        let source = workspace.open("nara.inspector", target).unwrap();
        let other = workspace
            .open("nara.inspector", EditorPanelTarget::ActiveDocument)
            .unwrap();
        let expected = workspace.layout().instance(source).unwrap().clone();
        let other_before = workspace.layout().instance(other).unwrap().clone();
        let instance_count = workspace.layout().instances().len();
        workspace.layout_mut().close(source);

        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        for _ in 0..4 {
            assert!(!render(&mut context, &mut workspace));
        }
        let rect = control_rect(&format!("panel-instance:{}", source.0))
            .expect("the existing closed Source Inspector must have an instance checkbox");
        let point = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        for visible in [true, false, true] {
            context.io_mut().add_mouse_pos_event(point);
            assert!(!render(&mut context, &mut workspace));
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
            let pressed = render(&mut context, &mut workspace);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
            let released = render(&mut context, &mut workspace);
            assert!(
                pressed || released,
                "the actual checkbox must report its visibility change"
            );
            let panel = workspace.layout().instance(source).unwrap();
            assert_eq!(panel.is_visible(), visible);
            assert!(panel.is_open());
            assert_eq!(panel.target(), expected.target());
            assert_eq!(workspace.layout().instances().len(), instance_count);
            assert_eq!(workspace.layout().instance(other), Some(&other_before));
        }
        assert_eq!(workspace.layout().instance(source), Some(&expected));
    }

    #[test]
    fn an_accepted_gesture_is_cancelled_when_its_native_panel_stops_painting() {
        use nara::{
            reflect::{
                ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
                ComponentValueKind,
            },
            scene::{PrefabAuthoringSession, PrefabDocument, SceneEntityId},
            tooling::{
                EditorAtomicFieldEdit, EditorFieldTarget, EditorFieldWrite, EditorGestureController,
            },
        };
        let workspace = EditorPanelWorkspace::default();
        let owner = workspace
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == "nara.inspector")
            .unwrap()
            .id();
        let revision =
            PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([])).revision();
        let baseline = EditorAtomicFieldEdit::new(
            EditorFieldTarget::new(
                EditorDocumentId::from_raw(1),
                revision,
                SceneEntityId::new("node").unwrap(),
                ComponentTypeId::new("nara.ui.UiNode"),
                ComponentSchemaVersion::ONE,
                ComponentFieldId::new("opacity"),
            ),
            [EditorFieldWrite::new(
                ComponentFieldId::new("opacity"),
                ComponentValueKind::F64,
                ComponentValue::f64(1.0).unwrap(),
            )
            .unwrap()],
        )
        .unwrap();
        let mut controller = EditorGestureController::default();
        let _ = controller.begin(baseline);
        let active = controller.active().unwrap();
        let mut presentation = ImguiPanelPresentation::default();
        presentation.gesture_owner = Some((
            owner,
            EditorPanelTarget::ActiveDocument,
            active.stamp().id(),
        ));
        let mut actions = Vec::new();
        presentation.painted.insert(owner);
        presentation.reconcile_gesture(&workspace, Some(active), &mut actions);
        assert!(actions.is_empty());
        presentation.painted.clear();
        presentation.reconcile_gesture(&workspace, Some(active), &mut actions);
        assert!(
            matches!(actions.as_slice(), [EditorAction::Gesture(EditorGestureAction::Cancel {
            gesture, reason: EditorGestureCancelReason::TargetRetired,
        })] if *gesture == active.stamp().id())
        );
    }
}
