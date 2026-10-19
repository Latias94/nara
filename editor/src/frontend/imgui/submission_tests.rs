use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

use dear_imgui_rs::{ConfigFlags, Context, Ui};
use nara::tooling::{EditorAction, EditorActionBatch, EditorActionOutcome, EditorNavigationAction};

use super::{
    ImguiEditorFrontend, ImguiEditorPanel, ImguiEditorPanelContext, ImguiEditorSubmission,
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId,
};
use crate::{
    EditorApplication, EditorLaunchOptions, EditorNativeComposition,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};

#[derive(Default)]
struct Evidence {
    count: usize,
    close_resolution: Option<nara::tooling::EditorCloseDecision>,
    queued: Vec<PanelSubmissionId>,
    finished: Vec<(PanelSubmissionId, PanelActionDispatch)>,
}

struct Probe(Rc<RefCell<Evidence>>);

impl ImguiEditorPanel for Probe {
    fn show(&mut self, _ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let Some(document) = context.model().document().map(|document| document.document) else {
            return;
        };
        let mut evidence = self.0.borrow_mut();
        for slot in 0..evidence.count {
            let action = if slot == 1 && evidence.close_resolution.is_some() {
                EditorAction::WorkspaceIntent(
                    context
                        .model()
                        .frame()
                        .workspace_intent()
                        .decision_prompt()
                        .unwrap()
                        .resolve(evidence.close_resolution.unwrap()),
                )
            } else {
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document,
                    entity: None,
                })
            };
            evidence.queued.push(context.submit(action).unwrap());
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        self.0.borrow_mut().finished.push((id, result));
    }
}

struct Harness {
    _project: super::interaction_tests::InteractionProject,
    application: EditorApplication,
    frontend: ImguiEditorFrontend,
    context: Context,
    panels: Vec<(EditorPanelInstanceId, Rc<RefCell<Evidence>>)>,
}

impl Harness {
    fn new(count: usize) -> Self {
        let project = super::interaction_tests::InteractionProject::new();
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
        application.native_shell_ready();
        for _ in 0..8 {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
        }
        assert!(application.current_frame().is_some());
        let mut composition = EditorNativeComposition::new();
        let mut evidence = Vec::new();
        for index in 0..count {
            let shared = Rc::new(RefCell::new(Evidence::default()));
            let factory = Rc::clone(&shared);
            let kind = format!("test.dispatch.{index}");
            composition
                .register_panel(
                    EditorPanelDescriptor::new(
                        &kind,
                        &kind,
                        EditorPanelMultiplicity::Multiple,
                        EditorPanelPlacement::Floating,
                        true,
                    ),
                    move || Probe(Rc::clone(&factory)),
                )
                .unwrap();
            evidence.push((kind, shared));
        }
        let frontend = composition.install(&mut application).unwrap();
        let panels = evidence
            .into_iter()
            .map(|(kind, evidence)| {
                let id = application
                    .panel_workspace()
                    .layout()
                    .instances()
                    .iter()
                    .find(|panel| panel.kind() == kind)
                    .unwrap()
                    .id();
                (id, evidence)
            })
            .collect();
        let mut context = Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE);
        context.io_mut().set_display_size([1800.0, 1100.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut harness = Self {
            _project: project,
            application,
            frontend,
            context,
            panels,
        };
        for _ in 0..3 {
            let candidate = harness.draw();
            harness
                .frontend
                .submit_and_drive(&mut harness.application, candidate, Duration::ZERO);
        }
        harness
    }

    fn draw(&mut self) -> ImguiEditorSubmission {
        let candidate = self
            .frontend
            .show(self.context.frame(), &mut self.application)
            .unwrap()
            .unwrap();
        let _ = self.context.render_legacy();
        candidate
    }

    fn submit(&mut self, candidate: ImguiEditorSubmission) {
        self.frontend
            .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
    }

    fn close_dirty_document(&mut self) -> nara::tooling::EditorDocumentId {
        use nara::{
            scene::{
                SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
                ScenePlacement, ScenePlacementOwner,
            },
            tooling::{EditorDocumentMutation, EditorWorkspaceIntent, EditorWorkspaceIntentAction},
        };
        let frame = self.application.current_frame().unwrap();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [
                    EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                        document: scene.document,
                        base_revision: scene.revision,
                        patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                            entity: SceneEntityRecord::new(
                                SceneEntityId::new("dispatch-dirty").unwrap(),
                            ),
                            placement: ScenePlacement::last(ScenePlacementOwner::Root),
                        }]),
                    }),
                    EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                        EditorWorkspaceIntent::CloseDocument {
                            document: scene.document,
                        },
                    )),
                ],
            ),
            Duration::ZERO,
        );
        assert!(
            self.application
                .current_frame()
                .unwrap()
                .workspace_intent()
                .decision_prompt()
                .is_some()
        );
        scene.document
    }
}

#[test]
fn two_panels_receive_only_their_own_multiple_ordered_action_results_once() {
    let mut harness = Harness::new(2);
    for (_, evidence) in &harness.panels {
        evidence.borrow_mut().count = 2;
    }
    let candidate = harness.draw();
    for (_, evidence) in &harness.panels {
        assert_eq!(evidence.borrow().queued.len(), 2);
        assert!(evidence.borrow().finished.is_empty());
    }
    let frame = harness.application.current_frame().unwrap();
    // Direct dispatch has no frontend observation authority. Keep the current published frame
    // here so this assertion remains independent of navigation's exact-frame admission gate.
    let direct = harness
        .application
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), []));
    assert!(!direct.actions_observed());
    assert!(std::sync::Arc::ptr_eq(
        &frame,
        &harness.application.current_frame().unwrap()
    ));
    assert!(
        harness
            .panels
            .iter()
            .all(|(_, evidence)| evidence.borrow().finished.is_empty())
    );
    harness.submit(candidate);
    let mut all = std::collections::BTreeSet::new();
    for (_, evidence) in &harness.panels {
        let evidence = evidence.borrow();
        assert_eq!(
            evidence.queued,
            evidence
                .finished
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        );
        assert!(evidence.finished.iter().all(|(id, outcome)| all.insert(*id)
            && matches!(
                outcome,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(_))
            )));
    }
    let frame = harness.application.current_frame().unwrap();
    // An unrelated direct batch cannot redeliver or settle frontend observations.
    harness
        .application
        .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
    assert!(
        harness
            .panels
            .iter()
            .all(|(_, evidence)| evidence.borrow().finished.len() == 2)
    );
}

#[test]
fn an_unrelated_application_drive_can_stale_a_candidate_without_settling_its_callbacks() {
    let mut harness = Harness::new(1);
    harness.panels[0].1.borrow_mut().count = 2;
    let candidate = harness.draw();
    let received = candidate.batch.frame_revision();
    harness
        .application
        .submit_and_drive(EditorActionBatch::new(received, []), Duration::ZERO);
    let expected = harness.application.current_frame().unwrap().revision();
    assert_ne!(received, expected);
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    harness.submit(candidate);
    {
        let evidence = harness.panels[0].1.borrow();
        assert_eq!(evidence.finished.len(), 2);
        assert_eq!(
            evidence.queued,
            evidence
                .finished
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        );
        assert!(evidence.finished.iter().all(|(_, outcome)| *outcome
            == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::BatchRejected(
                nara::tooling::EditorActionRejection::StaleFrame { expected, received }
            ))));
    }
    let fresh = harness.draw();
    harness.submit(fresh);
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 4);
    assert!(evidence.finished[2..].iter().all(|(_, outcome)| matches!(
        outcome,
        PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(_))
    )));
}

#[test]
fn foreign_and_superseded_candidates_with_the_same_frame_cannot_dispatch() {
    let mut harness = Harness::new(1);
    harness.panels[0].1.borrow_mut().count = 2;
    let foreign = harness.draw();
    let revision = foreign.batch.frame_revision();
    let before = harness.application.current_frame().unwrap();
    let update = ImguiEditorFrontend::new().submit_and_drive(
        &mut harness.application,
        foreign,
        Duration::ZERO,
    );
    assert!(!update.actions_observed());
    assert!(std::sync::Arc::ptr_eq(
        &before,
        &harness.application.current_frame().unwrap()
    ));
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    let stale = harness.draw();
    assert_eq!(revision, stale.batch.frame_revision());
    assert!(
        harness.panels[0]
            .1
            .borrow()
            .finished
            .iter()
            .all(|(_, result)| *result
                == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded))
    );
    let current = harness.draw();
    assert_eq!(revision, current.batch.frame_revision());
    let finished = harness.panels[0].1.borrow().finished.len();
    harness.submit(stale);
    assert_eq!(harness.panels[0].1.borrow().finished.len(), finished);
    harness.submit(current);
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 6);
    assert!(
        evidence.finished[4..]
            .iter()
            .all(|(_, result)| matches!(result, PanelActionDispatch::Dispatched(_)))
    );
}

#[test]
fn closing_filter_keeps_original_action_slots_for_each_callback() {
    let mut harness = Harness::new(1);
    harness.close_dirty_document();
    {
        let mut evidence = harness.panels[0].1.borrow_mut();
        evidence.count = 3;
        evidence.close_resolution = Some(nara::tooling::EditorCloseDecision::Cancel);
    }
    let candidate = harness.draw();
    harness.submit(candidate);
    let evidence = harness.panels[0].1.borrow();
    assert_eq!(evidence.finished.len(), 3);
    assert_eq!(
        evidence.finished[0].1,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert!(matches!(
        evidence.finished[1].1,
        PanelActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(_))
    ));
    assert_eq!(
        evidence.finished[2].1,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert_eq!(
        evidence.queued,
        evidence
            .finished
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn retargeting_before_dispatch_retires_the_original_action_and_recipient() {
    let mut harness = Harness::new(1);
    let frame = harness.application.current_frame().unwrap();
    let document = frame.workspace().active_scene.as_ref().unwrap().document;
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(nara::scene::SceneEntityId::new("preserved").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    harness.panels[0].1.borrow_mut().count = 1;
    let candidate = harness.draw();
    let id = harness.panels[0].0;
    let frame = harness.application.current_frame().unwrap();
    let target = EditorPanelTarget::Source {
        kind: crate::workspace::EditorPanelSourceKind::Scene,
        locator: "missing.scene.json".to_owned(),
    };
    harness
        .application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(id, target);
    harness.submit(candidate);
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    assert_eq!(
        frame.workspace().active_scene.as_ref().unwrap().selection,
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
    );
    assert!(!harness.frontend.presentation.widgets.contains_key(&id));
}

#[test]
fn a_document_closed_during_drive_cannot_deliver_to_its_retired_panel() {
    let mut harness = Harness::new(1);
    let document = harness.close_dirty_document();
    {
        let mut evidence = harness.panels[0].1.borrow_mut();
        evidence.count = 2;
        evidence.close_resolution = Some(nara::tooling::EditorCloseDecision::Discard);
    }
    let candidate = harness.draw();
    assert_eq!(harness.panels[0].1.borrow().queued.len(), 2);
    harness.submit(candidate);
    assert!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .is_none()
    );
    assert!(harness.panels[0].1.borrow().finished.is_empty());
    assert!(
        !harness
            .frontend
            .presentation
            .widgets
            .contains_key(&harness.panels[0].0)
    );
}

#[test]
fn queue_capacity_rejects_before_assigning_an_id_or_enqueuing() {
    use super::submission::{self, PanelOrigin, Recipient};
    let mut pending = Vec::new();
    let mut next = 0;
    let origin = PanelOrigin {
        instance: EditorPanelInstanceId(1),
        incarnation: Default::default(),
    };
    for slot in 0..submission::MAX_PANEL_SUBMISSIONS {
        assert!(
            submission::enqueue(
                &mut next,
                &mut pending,
                origin.clone(),
                slot,
                Recipient::Contribution
            )
            .is_ok()
        );
    }
    let old_next = next;
    assert_eq!(
        submission::enqueue(&mut next, &mut pending, origin, 0, Recipient::Contribution),
        Err(super::PanelSubmitError::Capacity)
    );
    assert_eq!(next, old_next);
    assert_eq!(pending.len(), submission::MAX_PANEL_SUBMISSIONS);
}
