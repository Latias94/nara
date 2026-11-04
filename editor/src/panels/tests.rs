#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]

use std::{
    cell::{Cell, RefCell},
    fs,
    rc::Rc,
    time::Instant,
};

use nara::{
    scene::{SceneDocument, SceneEntityId, SceneEntityRecord},
    tooling::{
        EditorAction, EditorActionOutcome, EditorCloseDecision, EditorDocumentId,
        EditorDocumentMutation, EditorDocumentQueryBasis, EditorEntityQuery,
        EditorEntityQueryLimits, EditorEntityQueryPosition, EditorNavigationAction,
        EditorWorkspaceIntent, EditorWorkspaceIntentAction,
    },
};

use crate::{
    EditorLaunchOptions,
    app::PersistenceFixture,
    workspace::{
        EditorNativeWindowId, EditorPanelMultiplicity, EditorPanelPlacement, EditorPanelTarget,
        EditorRenderViewId,
    },
};

use super::*;

#[path = "tests/workflows.rs"]
mod workflows;

#[derive(Default)]
struct Observations {
    created: Cell<usize>,
    dropped: RefCell<Vec<usize>>,
    finished: RefCell<Vec<(usize, PanelSubmissionId, PanelActionDispatch)>>,
}

struct Probe {
    serial: usize,
    visits: usize,
    observed: Rc<Observations>,
}

impl EditorPanel for Probe {
    fn action_finished(&mut self, id: PanelSubmissionId, dispatch: PanelActionDispatch) {
        self.observed
            .finished
            .borrow_mut()
            .push((self.serial, id, dispatch));
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.observed.dropped.borrow_mut().push(self.serial);
    }
}

fn descriptor(kind: &str) -> EditorPanelDescriptor {
    EditorPanelDescriptor::new(
        kind,
        "Independent panel",
        EditorPanelMultiplicity::Multiple,
        EditorPanelPlacement::Floating,
        true,
    )
}

fn register(
    host: &mut EditorPanelHost<Probe>,
    application: &mut EditorApplication,
    observed: &Rc<Observations>,
    kind: &str,
) -> EditorPanelInstanceId {
    let observed = Rc::clone(observed);
    host.register_panel(application, descriptor(kind), move || {
        let serial = observed.created.get() + 1;
        observed.created.set(serial);
        Probe {
            serial,
            visits: 0,
            observed: Rc::clone(&observed),
        }
    })
    .unwrap();
    application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == kind)
        .unwrap()
        .id()
}

fn open(fixture: &PersistenceFixture) -> EditorApplication {
    let mut application = EditorApplication::new(
        EditorLaunchOptions::new(fixture.project_root()).with_profile("desktop"),
    );
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() {
        assert!(
            Instant::now() < deadline,
            "ordinary project Open did not publish"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    application
}

fn application() -> (PersistenceFixture, EditorApplication) {
    let fixture = PersistenceFixture::authored_upgrade();
    fs::write(
        fixture.project_root().join("worlds/entry.scene.json"),
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new("alpha").unwrap()),
            SceneEntityRecord::new(SceneEntityId::new("beta").unwrap()),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let application = open(&fixture);
    (fixture, application)
}

fn current_basis(application: &EditorApplication) -> EditorDocumentQueryBasis {
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    application.document_query_basis(document).unwrap()
}

fn query(basis: &EditorDocumentQueryBasis, rows: usize) -> EditorEntityQuery {
    EditorEntityQuery {
        position: EditorEntityQueryPosition::Start(basis.clone()),
        limits: EditorEntityQueryLimits::new(rows, rows).unwrap(),
    }
}

fn dispatch(application: &mut EditorApplication, action: EditorAction) {
    let revision = application.current_frame().unwrap().revision();
    let submission =
        application.submit_and_drive(EditorActionBatch::new(revision, [action]), Duration::ZERO);
    assert_eq!(submission.actions().len(), 1);
    assert!(matches!(
        submission.actions()[0],
        crate::EditorApplicationActionDispatch::Dispatched(_)
    ));
}

fn close(application: &mut EditorApplication) {
    let mut update = application.request_exit();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !update.exit_ready() {
        assert!(
            Instant::now() < deadline,
            "panel test retained its application"
        );
        if let Some(frame) = application.current_frame()
            && let Some(prompt) = frame.workspace_intent().decision_prompt()
        {
            dispatch(
                application,
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard)),
            );
        }
        update = application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn registration_admits_descriptor_and_factory_atomically_without_a_toolkit() {
    let mut application = EditorApplication::new(EditorLaunchOptions::project_manager());
    let mut host = EditorPanelHost::<Probe>::new();
    let observed = Rc::new(Observations::default());
    let original = application.panel_workspace().layout().clone();
    assert!(matches!(
        host.register_panel(&mut application, descriptor(""), || unreachable!()),
        Err(EditorPanelRegistrationError::EmptyIdentity)
    ));
    assert!(matches!(
        host.register_panel(
            &mut application,
            descriptor("nara.inspector"),
            || unreachable!()
        ),
        Err(EditorPanelRegistrationError::Duplicate(_))
    ));
    assert_eq!(application.panel_workspace().layout(), &original);
    assert!(host.factories.is_empty());
    register(&mut host, &mut application, &observed, "test.semantic");
    let registered = application.panel_workspace().layout().clone();
    assert!(matches!(
        host.register_panel(
            &mut application,
            descriptor("test.semantic"),
            || unreachable!()
        ),
        Err(EditorPanelRegistrationError::Duplicate(_))
    ));
    assert_eq!(application.panel_workspace().layout(), &registered);
    assert_eq!(host.factories.len(), 1);
    assert!(host.prepare(&application, |_, _| unreachable!()).is_none());
    assert_eq!(observed.created.get(), 0);
}

#[test]
fn exact_results_are_ordered_once_and_keep_owned_query_observations() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let basis = current_basis(&application);
    let mut ids = Vec::new();
    let submission = host
        .prepare(&application, |panel, context| {
            assert_eq!(panel.serial, 1);
            for rows in [1, 2] {
                ids.push(
                    context
                        .submit(EditorAction::QueryDocumentEntities(query(&basis, rows)))
                        .unwrap(),
                );
            }
        })
        .unwrap();
    assert_ne!(ids[0], ids[1]);
    assert!(observed.finished.borrow().is_empty());
    host.submit_and_drive(&mut application, submission, Duration::ZERO);
    let finished = observed.finished.borrow().clone();
    assert_eq!(finished.len(), 2);
    for (index, (serial, id, result)) in finished.iter().enumerate() {
        assert_eq!(*serial, 1);
        assert_eq!(*id, ids[index]);
        let PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(page))) =
            result
        else {
            panic!("the original query did not receive its page: {result:?}");
        };
        assert_eq!(page.basis, basis);
        assert_eq!(page.entities.len(), index + 1);
    }
    let empty = host.prepare(&application, |_, _| {}).unwrap();
    host.submit_and_drive(&mut application, empty, Duration::ZERO);
    assert_eq!(*observed.finished.borrow(), finished);
    assert_eq!(current_basis(&application), basis);
    close(&mut application);
}

#[test]
fn foreign_and_superseded_candidates_cannot_settle_the_current_owner() {
    let (_fixture, mut application) = application();
    let mut first = EditorPanelHost::new();
    let mut second = EditorPanelHost::new();
    let first_observed = Rc::new(Observations::default());
    let second_observed = Rc::new(Observations::default());
    register(&mut first, &mut application, &first_observed, "test.first");
    register(
        &mut second,
        &mut application,
        &second_observed,
        "test.second",
    );
    let basis = current_basis(&application);
    let old = first
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .unwrap();
        })
        .unwrap();
    let current = first
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 2)))
                .unwrap();
        })
        .unwrap();
    assert!(matches!(
        first_observed.finished.borrow()[0].2,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
    ));
    let foreign = second
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .unwrap();
        })
        .unwrap();
    let before = application.current_frame().unwrap();
    assert_eq!(
        first.submit_and_drive(&mut application, old, Duration::from_secs(1)),
        EditorApplicationUpdate::idle()
    );
    assert_eq!(
        first.submit_and_drive(&mut application, foreign, Duration::from_secs(1)),
        EditorApplicationUpdate::idle()
    );
    assert!(Arc::ptr_eq(&before, &application.current_frame().unwrap()));
    assert_eq!(first_observed.finished.borrow().len(), 1);
    assert!(second_observed.finished.borrow().is_empty());
    first.submit_and_drive(&mut application, current, Duration::ZERO);
    assert_eq!(first_observed.finished.borrow().len(), 2);
    assert!(matches!(
        first_observed.finished.borrow()[1].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(_)))
    ));
    let second_current = second.prepare(&application, |_, _| {}).unwrap();
    assert_eq!(second_observed.finished.borrow().len(), 1);
    assert!(matches!(
        second_observed.finished.borrow()[0].2,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
    ));
    second.submit_and_drive(&mut application, second_current, Duration::ZERO);
    close(&mut application);
}

#[test]
fn dropping_candidates_or_the_host_does_not_dispatch_or_invent_callbacks() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let basis = current_basis(&application);
    let before = application.current_frame().unwrap();
    let dropped = host
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .unwrap();
        })
        .unwrap();
    drop(dropped);
    assert!(observed.finished.borrow().is_empty());
    assert!(Arc::ptr_eq(&before, &application.current_frame().unwrap()));
    let successor = host
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 2)))
                .unwrap();
        })
        .unwrap();
    assert_eq!(observed.finished.borrow().len(), 1);
    assert_eq!(
        observed.finished.borrow()[0].2,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
    );
    drop(successor);
    drop(host);
    assert_eq!(observed.finished.borrow().len(), 1);
    assert_eq!(*observed.dropped.borrow(), [1]);
    assert!(Arc::ptr_eq(&before, &application.current_frame().unwrap()));
    close(&mut application);
}

#[test]
fn stale_geometry_sensitive_batch_delivers_its_exact_rejection_once() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let basis = current_basis(&application);
    let stale = host
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: basis.document,
                        entity: Some(SceneEntityId::new("beta").unwrap()),
                    },
                ))
                .unwrap();
        })
        .unwrap();
    dispatch(
        &mut application,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: basis.document,
            entity: Some(SceneEntityId::new("alpha").unwrap()),
        }),
    );
    host.submit_and_drive(&mut application, stale, Duration::ZERO);
    assert_eq!(observed.finished.borrow().len(), 1);
    assert!(matches!(
        observed.finished.borrow()[0].2,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::BatchRejected(
            nara::tooling::EditorActionRejection::StaleFrame { .. }
        ))
    ));
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .selected_entity,
        Some(SceneEntityId::new("alpha").unwrap())
    );
    let _ = host.prepare(&application, |_, _| {}).unwrap();
    assert_eq!(observed.finished.borrow().len(), 1);
    close(&mut application);
}

#[test]
fn retiring_an_earlier_panel_keeps_the_surviving_original_action_slot() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    let first = register(&mut host, &mut application, &observed, "test.first");
    let second = register(&mut host, &mut application, &observed, "test.second");
    let basis = current_basis(&application);
    let before = application.current_frame().unwrap();
    let mut survivor = None;
    let submission = host
        .prepare(&application, |_, context| {
            if context.model().instance().id() == first {
                context
                    .submit(EditorAction::Navigate(
                        EditorNavigationAction::SelectEntity {
                            document: basis.document,
                            entity: Some(SceneEntityId::new("beta").unwrap()),
                        },
                    ))
                    .unwrap();
            } else {
                assert_eq!(context.model().instance().id(), second);
                survivor = Some(
                    context
                        .submit(EditorAction::QueryDocumentEntities(query(&basis, 2)))
                        .unwrap(),
                );
            }
        })
        .unwrap();
    assert_eq!(submission.0.ledger.submissions[1].slot, 1);
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_visible(first, false);
    host.submit_and_drive(&mut application, submission, Duration::ZERO);
    let finished = observed.finished.borrow();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].1, survivor.unwrap());
    assert!(matches!(
        finished[0].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(_)))
    ));
    let after = application.current_frame().unwrap();
    assert_eq!(after.workspace(), before.workspace());
    assert_eq!(*observed.dropped.borrow(), [1]);
    drop(finished);
    close(&mut application);
}

#[test]
fn relocation_preserves_each_typed_draft_but_hide_and_reopen_recreate_it() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    let first = register(&mut host, &mut application, &observed, "test.semantic");
    let second = application
        .panel_workspace_mut()
        .open("test.semantic", EditorPanelTarget::ActiveDocument)
        .unwrap();
    let _ = host
        .prepare(&application, |panel, _| panel.visits += 1)
        .unwrap();
    {
        let layout = application.panel_workspace_mut().layout_mut();
        assert!(layout.place(first, EditorPanelPlacement::Bottom));
        assert!(layout.set_native_window(first, EditorNativeWindowId(9)));
        assert!(layout.set_render_view(first, Some(EditorRenderViewId(19))));
        assert!(layout.set_render_view(second, None));
    }
    let _ = host
        .prepare(&application, |panel, _| {
            assert_eq!(panel.visits, 1);
            panel.visits += 1;
        })
        .unwrap();
    assert_eq!(observed.created.get(), 2);
    assert!(observed.dropped.borrow().is_empty());
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_visible(first, false);
    let _ = host
        .prepare(&application, |panel, context| {
            assert_eq!(context.model().instance().id(), second);
            assert_eq!(panel.visits, 2);
        })
        .unwrap();
    assert_eq!(*observed.dropped.borrow(), [1]);
    application.panel_workspace_mut().layout_mut().reopen(first);
    let _ = host
        .prepare(&application, |panel, context| {
            if context.model().instance().id() == first {
                assert_eq!(panel.serial, 3);
                assert_eq!(panel.visits, 0);
            } else {
                assert_eq!(panel.serial, 2);
                assert_eq!(panel.visits, 2);
            }
        })
        .unwrap();
    assert_eq!(observed.created.get(), 3);
    close(&mut application);
}

#[test]
fn hide_reopen_and_target_round_trips_before_dispatch_cannot_revive_old_actions() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    let instance = register(&mut host, &mut application, &observed, "test.semantic");
    let basis = current_basis(&application);
    let initial = application.current_frame().unwrap();
    for operation in [0, 1, 2] {
        let candidate = host
            .prepare(&application, |panel, context| {
                assert_eq!(panel.serial, operation + 1);
                context
                    .submit(EditorAction::Navigate(
                        EditorNavigationAction::SelectEntity {
                            document: basis.document,
                            entity: Some(SceneEntityId::new("beta").unwrap()),
                        },
                    ))
                    .unwrap();
            })
            .unwrap();
        let layout = application.panel_workspace_mut().layout_mut();
        match operation {
            0 => {
                assert!(layout.set_visible(instance, false));
                assert!(layout.reopen(instance));
            }
            1 => {
                assert!(layout.close(instance));
                assert!(layout.reopen(instance));
            }
            _ => {
                assert!(layout.set_target(instance, EditorPanelTarget::Workspace));
                assert!(layout.set_target(instance, EditorPanelTarget::ActiveDocument));
            }
        }
        host.submit_and_drive(&mut application, candidate, Duration::ZERO);
        assert_eq!(observed.dropped.borrow().len(), operation + 1);
        assert!(observed.finished.borrow().is_empty());
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            initial.workspace()
        );
    }
    let _ = host
        .prepare(&application, |panel, context| {
            assert_eq!(panel.serial, 4);
            assert_eq!(context.model().instance().id(), instance);
        })
        .unwrap();
    assert_eq!(observed.created.get(), 4);
    close(&mut application);
}

#[test]
fn layout_reset_and_identical_restore_retire_queued_original_incarnations() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "a.semantic");
    let basis = current_basis(&application);
    let old = host
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .unwrap();
        })
        .unwrap();
    application.panel_workspace_mut().reset();
    host.submit_and_drive(&mut application, old, Duration::ZERO);
    assert_eq!(*observed.dropped.borrow(), [1]);
    assert!(observed.finished.borrow().is_empty());
    let restored = host
        .prepare(&application, |panel, context| {
            assert_eq!(panel.serial, 2);
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .unwrap();
        })
        .unwrap();
    let layout = application.panel_workspace().layout().clone();
    application
        .panel_workspace_mut()
        .restore(layout.clone(), "test-independent", 1, &[]);
    assert_eq!(application.panel_workspace().layout(), &layout);
    host.submit_and_drive(&mut application, restored, Duration::ZERO);
    assert_eq!(*observed.dropped.borrow(), [1, 2]);
    assert!(observed.finished.borrow().is_empty());
    let _ = host
        .prepare(&application, |panel, _| assert_eq!(panel.serial, 3))
        .unwrap();
    assert_eq!(observed.created.get(), 3);
    close(&mut application);
}

#[test]
fn project_replacement_rejects_only_the_matching_original_candidate() {
    let (fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let basis = current_basis(&application);
    let old = host
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .unwrap();
        })
        .unwrap();
    let generation = application.workspace_generation();
    assert!(application.request_project_replacement(
        fixture.project_root().to_owned(),
        Some("desktop".to_owned()),
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.workspace_generation().is_none()
        || application.workspace_generation() == generation
    {
        assert!(
            Instant::now() < deadline,
            "replacement project did not publish"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let replacement = application.current_frame().unwrap();
    assert_eq!(
        host.submit_and_drive(&mut application, old, Duration::from_secs(1)),
        EditorApplicationUpdate::idle()
    );
    assert!(Arc::ptr_eq(
        &replacement,
        &application.current_frame().unwrap()
    ));
    assert_eq!(*observed.dropped.borrow(), [1]);
    assert!(observed.finished.borrow().is_empty());
    assert!(host.submissions.pending.is_none());
    let current = host
        .prepare(&application, |panel, context| {
            assert_eq!(panel.serial, 2);
            let basis = context
                .document_query_basis(context.model().document().unwrap().document)
                .unwrap();
            context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 2)))
                .unwrap();
        })
        .unwrap();
    host.submit_and_drive(&mut application, current, Duration::ZERO);
    assert_eq!(observed.finished.borrow().len(), 1);
    assert_eq!(observed.finished.borrow()[0].0, 2);
    close(&mut application);
}

#[test]
fn closing_filter_reports_the_original_hole_and_delivers_later_resolution() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let basis = current_basis(&application);
    let patch = crate::app::named_root_patch(&application, basis.document, "dirty-root");
    dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
            document: basis.document,
            base_revision: basis.revision,
            patch,
        }),
    );
    let basis = current_basis(&application);
    dispatch(
        &mut application,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
            EditorWorkspaceIntent::Exit,
        )),
    );
    let prompt = application
        .current_frame()
        .unwrap()
        .workspace_intent()
        .decision_prompt()
        .unwrap();
    let mut ids = Vec::new();
    let submission = host
        .prepare(&application, |_, context| {
            ids.push(
                context
                    .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                    .unwrap(),
            );
            ids.push(
                context
                    .submit(EditorAction::WorkspaceIntent(
                        prompt.resolve(EditorCloseDecision::Cancel),
                    ))
                    .unwrap(),
            );
        })
        .unwrap();
    host.submit_and_drive(&mut application, submission, Duration::ZERO);
    let finished = observed.finished.borrow();
    assert_eq!(finished.len(), 2);
    assert_eq!(finished[0].1, ids[0]);
    assert_eq!(finished[1].1, ids[1]);
    assert_eq!(
        finished[0].2,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert!(matches!(
        finished[1].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(_))
    ));
    assert!(
        application
            .current_frame()
            .unwrap()
            .workspace_intent()
            .intent()
            .is_none()
    );
    assert_eq!(current_basis(&application), basis);
    drop(finished);
    close(&mut application);
}
