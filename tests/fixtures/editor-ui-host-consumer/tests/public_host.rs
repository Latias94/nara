#![forbid(unsafe_code)]
#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]

use std::{
    fs,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    reflect::{ComponentTypeId, ComponentValue},
    scene::{SceneDocumentCandidate, ScenePlacement, ScenePlacementParent},
    tooling::{
        EditorAction, EditorActionOutcome, EditorAuthoringProjection, EditorAuthoringQuery,
        EditorAuthoringQueryLimits, EditorAuthoringQueryPosition, EditorAuthoringQueryTarget,
        EditorAuthoringRow, EditorCloseDecision, EditorDocumentMutation, EditorEntityQuery,
        EditorEntityQueryLimits, EditorEntityQueryPosition, EditorNavigationAction,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPersistenceView, EditorPropertyValue, EditorReopenTarget, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction,
    },
};
use nara_editor::{
    EditorApplication, EditorApplicationState, EditorApplicationUpdate, EditorPanelHost,
    EditorPanelSubmission, PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId,
    PanelSubmitError,
    workspace::{EditorNativeWindowId, EditorPanelPlacement, EditorRenderViewId},
};
use nara_editor_ui_host_consumer::{
    COMPONENT, DraftPanel, Evidence, FIELD, INITIAL,
    project::{
        self, Application, Project, SOURCE, current, dispatch, entity, open_secondary, persistence,
        property, query, register,
    },
};
use nara_text::{TextEdit, bundled_font_face};
use nara_ui_kernel::{UiCompiler, UiEnvironment, UiSemanticRole, UiSurfaceId, UiTheme};

fn through(
    host: &mut EditorPanelHost<DraftPanel>,
    application: &EditorApplication,
    action: EditorAction,
) -> (EditorPanelSubmission, PanelSubmissionId) {
    let mut action = Some(action);
    let mut id = None;
    let submission = host
        .prepare(application, |_, context| {
            id = Some(context.submit(action.take().unwrap()).unwrap());
        })
        .unwrap();
    (submission, id.unwrap())
}

fn applied(evidence: &Evidence, id: PanelSubmissionId) {
    let finished = evidence.finished.borrow();
    let results: Vec<_> = finished
        .iter()
        .filter(|(_, submitted, _)| *submitted == id)
        .collect();
    assert_eq!(results.len(), 1);
    assert!(
        matches!(&results[0].2, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied),
        "{:?}",
        results[0]
    );
}

fn caption(application: &EditorApplication, document: nara::tooling::EditorDocumentId) -> String {
    let basis = application.document_query_basis(document).unwrap();
    let read = application
        .read_document_property(property(&basis))
        .unwrap();
    let EditorPropertyValue::Present(ComponentValue::String(value)) = read.value else {
        panic!("{read:?}")
    };
    value
}

#[test]
fn controlled_panel_edits_nonactive_source_with_exact_results_history_and_persistence() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let secondary = open_secondary(&mut application);
    let navigation = current(&application);
    let active = navigation.workspace().active_document.unwrap();
    assert_ne!(active, secondary);
    let disk = fs::read(project.root().join(SOURCE)).unwrap();
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    register(&mut host, &mut application, &evidence, "consumer.caption");
    let original = application.document_query_basis(secondary).unwrap();
    let mut expected = Vec::new();
    let prepared = host.prepare(&application, |panel, context| {
        assert!(context.model().document().is_none());
        assert!(context.source_tool_review().unwrap().is_none());
        let basis = context.document_query_basis(secondary).unwrap();
        assert_eq!(basis, original);
        let first = context.query_document_entities(query(&basis, 1)).unwrap();
        assert_eq!(first.entities, [entity("caption")]);
        assert_eq!(first.scanned_records, 1);
        let next = context.query_document_entities(EditorEntityQuery {
            position: EditorEntityQueryPosition::Continue(first.next.clone().unwrap()),
            limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
        }).unwrap();
        assert_eq!(next.entities, [entity("tail")]);
        let metadata = context.query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                basis: basis.clone(), entity: entity("caption"),
                projection: EditorAuthoringProjection::Roots { component: ComponentTypeId::new(COMPONENT) },
            }),
            limits: EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap(),
        }).unwrap();
        assert!(matches!(metadata.rows.as_slice(), [EditorAuthoringRow::Root { field, edit_capability: true, .. }] if field.as_str() == FIELD));
        assert!(metadata.next.is_none());
        let read = context.read_document_property(property(&basis)).unwrap();
        assert_eq!(read.value, EditorPropertyValue::Present(ComponentValue::String(INITIAL.into())));
        let token = panel.replace_for_cpu("external draft").unwrap();
        panel.edit(token, &TextEdit::Undo).unwrap();
        assert_eq!(panel.draft(), INITIAL);
        panel.edit(token, &TextEdit::Redo).unwrap();
        let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
        for revision in [1, 2] {
            let frame = compiler.compile(&panel.compose(revision).unwrap(),
                UiEnvironment::new(UiSurfaceId::new(91).unwrap(), 400.0, 180.0, 1.0, 1, revision, 1, revision).unwrap(),
                UiTheme::default()).unwrap();
            assert!(frame.diagnostics().is_empty());
            assert!(frame.semantics().iter().any(|node| node.role() == UiSemanticRole::TextInput && node.value() == Some("external draft")));
        }
        let cancelled = panel.prepare_edit(context, &basis, entity("caption")).unwrap();
        drop(cancelled);
        let prepared = panel.prepare_edit(context, &basis, entity("caption")).unwrap();
        assert_eq!(prepared.patch().operations.len(), 1);
        let read_id = context.submit(EditorAction::ReadDocumentProperty(property(&basis))).unwrap();
        expected.push(read_id);
        expected.push(context.commit_authoring_edit(prepared).unwrap());
    }).unwrap();
    assert_eq!(current(&application).workspace(), navigation.workspace());
    assert_eq!(
        application.document_query_basis(secondary).unwrap(),
        original
    );
    assert!(evidence.finished.borrow().is_empty());
    assert_eq!(fs::read(project.root().join(SOURCE)).unwrap(), disk);
    host.submit_and_drive(&mut application, prepared, Duration::ZERO);
    {
        let finished = evidence.finished.borrow();
        assert_eq!(finished.len(), 2);
        assert_eq!(
            finished.iter().map(|(_, id, _)| *id).collect::<Vec<_>>(),
            expected
        );
        assert!(
            matches!(&finished[0].2, PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentProperty(Ok(read))) if read.value == EditorPropertyValue::Present(ComponentValue::String(INITIAL.into())))
        );
    }
    applied(&evidence, expected[1]);
    assert_eq!(caption(&application, secondary), "external draft");
    assert_eq!(
        current(&application).workspace().active_document,
        Some(active)
    );
    assert_eq!(
        current(&application).workspace().active_scene,
        navigation.workspace().active_scene
    );
    assert_eq!(
        current(&application)
            .workspace()
            .document(secondary)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    for (undo, value) in [(true, INITIAL), (false, "external draft")] {
        let basis = application.document_query_basis(secondary).unwrap();
        let mutation = if undo {
            EditorDocumentMutation::Undo {
                document: secondary,
                base_revision: basis.revision,
            }
        } else {
            EditorDocumentMutation::Redo {
                document: secondary,
                base_revision: basis.revision,
            }
        };
        let (batch, id) = through(&mut host, &application, EditorAction::Mutate(mutation));
        host.submit_and_drive(&mut application, batch, Duration::ZERO);
        applied(&evidence, id);
        assert_eq!(caption(&application, secondary), value);
    }
    assert_eq!(fs::read(project.root().join(SOURCE)).unwrap(), disk);
    let basis = application.document_query_basis(secondary).unwrap();
    let persistence_callbacks = evidence.finished.borrow().len();
    let (batch, save_id) = through(
        &mut host,
        &application,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document: secondary,
            base_revision: basis.revision,
        }),
    );
    host.submit_and_drive(&mut application, batch, Duration::ZERO);
    let save_attempt = current(&application).persistence().attempt().unwrap();
    let save_result = persistence(&mut application);
    assert!(
        matches!(save_result, EditorPersistenceResult::Saved { document, revision, .. } if document == secondary && revision == basis.revision)
    );
    assert_eq!(
        current(&application).persistence(),
        EditorPersistenceView::Terminal {
            attempt: save_attempt,
            result: save_result,
        }
    );
    let saved_bytes = fs::read(project.root().join(SOURCE)).unwrap();
    let saved = SceneDocumentCandidate::decode_json_bytes(&saved_bytes)
        .unwrap()
        .into_composition_source()
        .unwrap();
    let mut expected_source = project::source();
    expected_source
        .entities
        .iter_mut()
        .find(|record| record.id == entity("caption"))
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .value = ComponentValue::map([(FIELD, ComponentValue::String("external draft".into()))]);
    assert_eq!(saved, expected_source);
    let (batch, save_acknowledge_id) = through(
        &mut host,
        &application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
            attempt: save_attempt,
        }),
    );
    host.submit_and_drive(&mut application, batch, Duration::ZERO);
    assert_eq!(
        current(&application).persistence(),
        EditorPersistenceView::Idle
    );
    let (batch, reopen_id) = through(
        &mut host,
        &application,
        EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document: secondary,
                base_revision: basis.revision,
            },
        }),
    );
    host.submit_and_drive(&mut application, batch, Duration::ZERO);
    let reopen_attempt = current(&application).persistence().attempt().unwrap();
    assert_ne!(reopen_attempt, save_attempt);
    let reopen_result = persistence(&mut application);
    assert_eq!(
        current(&application).persistence(),
        EditorPersistenceView::Terminal {
            attempt: reopen_attempt,
            result: reopen_result,
        }
    );
    let EditorPersistenceResult::Opened {
        document: reopened,
        revision: reopened_revision,
        ..
    } = reopen_result
    else {
        panic!("Reopen did not complete")
    };
    assert_eq!(caption(&application, reopened), "external draft");
    let reopened_basis = application.document_query_basis(reopened).unwrap();
    assert_eq!(reopened_basis.revision, reopened_revision);
    assert_ne!(
        reopened_basis.revision.source_id(),
        basis.revision.source_id()
    );
    assert!(
        application
            .read_document_property(property(&basis))
            .is_err()
    );
    assert_eq!(
        current(&application)
            .workspace()
            .document(reopened)
            .unwrap()
            .history
            .undo_depth,
        0
    );
    assert_eq!(
        current(&application).workspace().active_document,
        Some(active)
    );
    assert_eq!(fs::read(project.root().join(SOURCE)).unwrap(), saved_bytes);
    let (batch, reopen_acknowledge_id) = through(
        &mut host,
        &application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
            attempt: reopen_attempt,
        }),
    );
    host.submit_and_drive(&mut application, batch, Duration::ZERO);
    assert_eq!(
        current(&application).persistence(),
        EditorPersistenceView::Idle
    );
    assert_eq!(
        &evidence.finished.borrow()[persistence_callbacks..],
        [
            (
                1,
                save_id,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
                    EditorPersistenceRequestResult::Accepted {
                        attempt: save_attempt,
                    },
                )),
            ),
            (
                1,
                save_acknowledge_id,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
                    EditorPersistenceRequestResult::Acknowledged {
                        attempt: save_attempt,
                    },
                )),
            ),
            (
                1,
                reopen_id,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
                    EditorPersistenceRequestResult::Accepted {
                        attempt: reopen_attempt,
                    },
                )),
            ),
            (
                1,
                reopen_acknowledge_id,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
                    EditorPersistenceRequestResult::Acknowledged {
                        attempt: reopen_attempt,
                    },
                )),
            ),
        ]
    );
    let callbacks = evidence.finished.borrow().len();
    drop(
        host.prepare(&application, |panel, _| {
            assert_eq!(panel.draft(), "external draft")
        })
        .unwrap(),
    );
    assert_eq!(evidence.created.get(), 1);
    assert_eq!(evidence.finished.borrow().len(), callbacks);
    assert!(application.close());
}

#[test]
fn relocation_preserves_real_text_but_invisible_lifecycle_round_trips_retire_it() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let secondary = open_secondary(&mut application);
    let basis = application.document_query_basis(secondary).unwrap();
    let original_property = application
        .read_document_property(property(&basis))
        .unwrap();
    assert_eq!(
        original_property.value,
        EditorPropertyValue::Present(ComponentValue::String(INITIAL.into()))
    );
    let before = current(&application);
    let disk = fs::read(project.root().join(SOURCE)).unwrap();
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    let instance = register(&mut host, &mut application, &evidence, "consumer.draft");
    let mut key = None;
    let mut token = None;
    let mut retained = None;
    drop(
        host.prepare(&application, |panel, _| {
            token = Some(panel.replace_for_cpu("retained draft").unwrap());
            panel
                .edit(
                    token.unwrap(),
                    &TextEdit::Preedit {
                        text: "candidate".into(),
                        cursor: Some((0, 9)),
                    },
                )
                .unwrap();
            key = Some(panel.field_key().clone());
            retained = Some(panel.compose(1).unwrap());
        })
        .unwrap(),
    );
    {
        let layout = application.panel_workspace_mut().layout_mut();
        assert!(layout.place(instance, EditorPanelPlacement::Bottom));
        assert!(layout.set_native_window(instance, EditorNativeWindowId(9)));
        assert!(layout.set_render_view(instance, Some(EditorRenderViewId(11))));
    }
    drop(
        host.prepare(&application, |panel, _| {
            assert_eq!(panel.draft(), "retained draft");
            assert!(panel.display_text().contains("candidate"));
            assert_eq!(panel.field_key(), key.as_ref().unwrap());
            assert_eq!(panel.input_token(), token);
            assert_eq!(&panel.compose(1).unwrap(), retained.as_ref().unwrap());
        })
        .unwrap(),
    );
    for close in [false, true] {
        let candidate = host
            .prepare(&application, |panel, context| {
                if close {
                    token = Some(panel.replace_for_cpu("retained draft").unwrap());
                    panel
                        .edit(
                            token.unwrap(),
                            &TextEdit::Preedit {
                                text: "candidate".into(),
                                cursor: Some((0, 9)),
                            },
                        )
                        .unwrap();
                    key = Some(panel.field_key().clone());
                }
                assert_eq!(panel.draft(), "retained draft");
                assert!(panel.display_text().contains("candidate"));
                assert_eq!(context.document_query_basis(secondary).unwrap(), basis);
                let prepared = panel
                    .prepare_edit(context, &basis, entity("caption"))
                    .unwrap();
                assert_eq!(prepared.base_revision(), basis.revision);
                assert_eq!(prepared.patch().operations.len(), 1);
                context.commit_authoring_edit(prepared).unwrap();
            })
            .unwrap();
        let callbacks = evidence.finished.borrow().len();
        let retired = evidence.created.get();
        {
            let layout = application.panel_workspace_mut().layout_mut();
            if close {
                assert!(layout.close(instance));
            } else {
                assert!(layout.set_visible(instance, false));
            }
            assert!(layout.reopen(instance));
        }
        host.submit_and_drive(&mut application, candidate, Duration::ZERO);
        assert_eq!(evidence.finished.borrow().len(), callbacks);
        assert!(evidence.dropped.borrow().contains(&retired));
        drop(
            host.prepare(&application, |panel, _| {
                assert_eq!(panel.serial(), retired + 1);
                assert_eq!(panel.draft(), INITIAL);
                assert_eq!(panel.display_text(), INITIAL);
                assert!(panel.input_token().is_none());
                assert_ne!(panel.field_key(), key.as_ref().unwrap());
                let current = panel.focus_for_cpu().unwrap();
                assert_ne!(Some(current), token);
                assert!(
                    panel
                        .edit(token.unwrap(), &TextEdit::Commit("late".into()))
                        .is_err()
                );
            })
            .unwrap(),
        );
        assert_eq!(evidence.finished.borrow().len(), callbacks);
        assert_eq!(application.document_query_basis(secondary).unwrap(), basis);
        assert_eq!(
            application
                .read_document_property(property(&basis))
                .unwrap(),
            original_property
        );
        let after = current(&application);
        let document = after.workspace().document(secondary).unwrap();
        assert_eq!(document.revision, basis.revision);
        assert_eq!(
            document.history,
            before.workspace().document(secondary).unwrap().history
        );
        assert_eq!(after.workspace(), before.workspace());
        assert_eq!(fs::read(project.root().join(SOURCE)).unwrap(), disk);
    }
    assert_eq!(evidence.created.get(), 3);
    assert_eq!(*evidence.dropped.borrow(), [1, 2]);
    assert!(application.close());
}

#[test]
fn foreign_superseded_and_dropped_candidates_never_dispatch_or_steal_callbacks() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let basis = application
        .document_query_basis(current(&application).workspace().active_document.unwrap())
        .unwrap();
    let mut first = EditorPanelHost::new();
    let mut second = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    let foreign_evidence = Rc::new(Evidence::default());
    register(&mut first, &mut application, &evidence, "consumer.first");
    register(
        &mut second,
        &mut application,
        &foreign_evidence,
        "consumer.second",
    );
    let (old, old_id) = through(
        &mut first,
        &application,
        EditorAction::QueryDocumentEntities(query(&basis, 1)),
    );
    let (current_candidate, current_id) = through(
        &mut first,
        &application,
        EditorAction::QueryDocumentEntities(query(&basis, 2)),
    );
    assert_eq!(
        evidence.finished.borrow().as_slice(),
        [(
            1,
            old_id,
            PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
        )]
    );
    let (foreign, foreign_id) = through(
        &mut second,
        &application,
        EditorAction::QueryDocumentEntities(query(&basis, 1)),
    );
    let before = current(&application);
    assert_eq!(
        first.submit_and_drive(&mut application, old, Duration::from_secs(1)),
        EditorApplicationUpdate::idle()
    );
    assert_eq!(
        first.submit_and_drive(&mut application, foreign, Duration::from_secs(1)),
        EditorApplicationUpdate::idle()
    );
    assert!(Arc::ptr_eq(&before, &current(&application)));
    assert!(foreign_evidence.finished.borrow().is_empty());
    first.submit_and_drive(&mut application, current_candidate, Duration::ZERO);
    assert_eq!(evidence.finished.borrow().len(), 2);
    assert_eq!(evidence.finished.borrow()[1].1, current_id);
    assert!(
        matches!(&evidence.finished.borrow()[1].2, PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(page))) if page.basis == basis)
    );
    let (dropped, _) = through(
        &mut first,
        &application,
        EditorAction::QueryDocumentEntities(query(&basis, 1)),
    );
    let before = current(&application);
    drop(dropped);
    assert!(Arc::ptr_eq(&before, &current(&application)));
    assert_eq!(evidence.finished.borrow().len(), 2);
    drop(first);
    assert_eq!(*evidence.dropped.borrow(), [1]);
    drop(second.prepare(&application, |_, _| {}).unwrap());
    assert_eq!(
        foreign_evidence.finished.borrow().as_slice(),
        [(
            1,
            foreign_id,
            PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
        )]
    );
    assert!(application.close());
}

#[test]
fn earlier_panel_retirement_preserves_later_original_result_slot() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let basis = application
        .document_query_basis(current(&application).workspace().active_document.unwrap())
        .unwrap();
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    let first = register(&mut host, &mut application, &evidence, "consumer.first");
    let second = register(&mut host, &mut application, &evidence, "consumer.second");
    let mut survivor = None;
    let mut order = Vec::new();
    let before = current(&application);
    let batch = host
        .prepare(&application, |_, context| {
            let instance = context.model().instance().id();
            order.push(instance);
            let action = if instance == first {
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: basis.document,
                    entity: Some(entity("observer")),
                })
            } else {
                assert_eq!(instance, second);
                EditorAction::QueryDocumentEntities(query(&basis, 1))
            };
            let id = context.submit(action).unwrap();
            if instance == second {
                survivor = Some(id);
            }
        })
        .unwrap();
    assert_eq!(order, [first, second]);
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_visible(first, false);
    host.submit_and_drive(&mut application, batch, Duration::ZERO);
    let finished = evidence.finished.borrow();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].1, survivor.unwrap());
    assert!(
        matches!(&finished[0].2, PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(page))) if page.entities == [entity("observer")])
    );
    assert_eq!(current(&application).workspace(), before.workspace());
    assert_eq!(*evidence.dropped.borrow(), [1]);
    drop(finished);
    assert!(application.close());
}

#[test]
fn layout_reset_restore_and_project_replacement_retire_queued_incarnations() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    register(&mut host, &mut application, &evidence, "consumer.draft");
    for restore in [false, true] {
        let basis = application
            .document_query_basis(current(&application).workspace().active_document.unwrap())
            .unwrap();
        let (candidate, _) = through(
            &mut host,
            &application,
            EditorAction::QueryDocumentEntities(query(&basis, 1)),
        );
        if restore {
            let saved = application.panel_workspace().layout().clone();
            application
                .panel_workspace_mut()
                .restore(saved, "external-ui", 1, &[]);
        } else {
            application.panel_workspace_mut().reset();
        }
        host.submit_and_drive(&mut application, candidate, Duration::ZERO);
        assert!(evidence.finished.borrow().is_empty());
    }
    let basis = application
        .document_query_basis(current(&application).workspace().active_document.unwrap())
        .unwrap();
    let (candidate, _) = through(
        &mut host,
        &application,
        EditorAction::QueryDocumentEntities(query(&basis, 1)),
    );
    assert!(application.request_project_replacement(project.root().to_owned(), None));
    assert_eq!(
        application.state(),
        EditorApplicationState::OpeningReplacement
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.state() == EditorApplicationState::OpeningReplacement {
        assert!(
            Instant::now() < deadline,
            "project replacement did not publish"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        application.state(),
        EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded
    ));
    assert!(application.current_frame().is_some());
    host.submit_and_drive(&mut application, candidate, Duration::ZERO);
    assert!(evidence.finished.borrow().is_empty());
    assert_eq!(*evidence.dropped.borrow(), [1, 2, 3]);
    drop(
        host.prepare(&application, |panel, _| {
            assert_eq!(panel.serial(), 4);
            assert_eq!(panel.draft(), INITIAL);
        })
        .unwrap(),
    );
    assert!(application.close());
}

#[test]
fn closing_filter_keeps_the_query_hole_and_later_cancel_result_exact() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let document = open_secondary(&mut application);
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    register(&mut host, &mut application, &evidence, "consumer.caption");
    let basis = application.document_query_basis(document).unwrap();
    let dirty = host
        .prepare(&application, |panel, context| {
            panel.replace_for_cpu("dirty").unwrap();
            let prepared = panel
                .prepare_edit(context, &basis, entity("caption"))
                .unwrap();
            context.commit_authoring_edit(prepared).unwrap();
        })
        .unwrap();
    host.submit_and_drive(&mut application, dirty, Duration::ZERO);
    let basis = application.document_query_basis(document).unwrap();
    dispatch(
        &mut application,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
            EditorWorkspaceIntent::Exit,
        )),
    );
    let frame = current(&application);
    let prompt = frame.workspace_intent().decision_prompt().unwrap();
    let callbacks = evidence.finished.borrow().len();
    let mut ids = Vec::new();
    let candidate = host
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
    host.submit_and_drive(&mut application, candidate, Duration::ZERO);
    let finished = evidence.finished.borrow();
    assert_eq!(finished.len(), callbacks + 2);
    assert_eq!(finished[callbacks].1, ids[0]);
    assert_eq!(
        finished[callbacks].2,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing)
    );
    assert_eq!(finished[callbacks + 1].1, ids[1]);
    assert!(matches!(
        finished[callbacks + 1].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::WorkspaceIntent(_))
    ));
    assert!(current(&application).workspace_intent().intent().is_none());
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    drop(finished);
    assert!(application.close());
}

#[test]
fn capacity_returns_original_prepared_edit_and_construction_for_retry() {
    let project = Project::new();
    let mut application = Application::open(&project);
    let document = open_secondary(&mut application);
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    register(&mut host, &mut application, &evidence, "consumer.caption");
    let basis = application.document_query_basis(document).unwrap();
    let mut retained = None;
    let mut construction = None;
    let mut local = None;
    let mut allocated = None;
    let mut count = 0;
    let full = host
        .prepare(&application, |panel, context| {
            panel.replace_for_cpu("retry").unwrap();
            let prepared = panel
                .prepare_edit(context, &basis, entity("caption"))
                .unwrap();
            let patch = prepared.patch().clone();
            let inverse = prepared.inverse().clone();
            let mut draft = context.begin_authoring_construction(&basis).unwrap();
            let created = draft
                .create_entity(ScenePlacement::last(ScenePlacementParent::Root))
                .unwrap();
            let prepared_construction = context
                .prepare_authoring_construction(&basis, &draft)
                .unwrap();
            allocated = prepared_construction.entity(&created).cloned();
            while context
                .submit(EditorAction::QueryDocumentEntities(query(&basis, 1)))
                .is_ok()
            {
                count += 1;
                assert!(
                    count <= 1024,
                    "the documented finite action limit was not enforced"
                );
            }
            assert_eq!(count, 1024);
            let (error, returned) = context.commit_authoring_edit(prepared).unwrap_err();
            assert_eq!(error, PanelSubmitError::Capacity);
            assert_eq!(returned.patch(), &patch);
            assert_eq!(returned.inverse(), &inverse);
            assert_eq!(returned.base_revision(), basis.revision);
            retained = Some(returned);
            let patch = prepared_construction.patch().clone();
            let (error, returned) = context
                .commit_authoring_construction(prepared_construction)
                .unwrap_err();
            assert_eq!(error, PanelSubmitError::Capacity);
            assert_eq!(returned.patch(), &patch);
            assert_eq!(returned.entity(&created), allocated.as_ref());
            construction = Some(returned);
            local = Some(created);
        })
        .unwrap();
    drop(full);
    assert!(evidence.finished.borrow().is_empty());
    assert_eq!(caption(&application, document), INITIAL);
    let mut expected_id = None;
    let retry = host
        .prepare(&application, |_, context| {
            expected_id = Some(
                context
                    .commit_authoring_construction(construction.take().unwrap())
                    .unwrap(),
            );
        })
        .unwrap();
    assert_eq!(evidence.finished.borrow().len(), count);
    assert!(
        evidence
            .finished
            .borrow()
            .iter()
            .all(|(_, _, result)| *result
                == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded))
    );
    host.submit_and_drive(&mut application, retry, Duration::ZERO);
    {
        let finished = evidence.finished.borrow();
        assert_eq!(finished.len(), count + 1);
        assert_eq!(finished[count].1, expected_id.unwrap());
        let PanelActionDispatch::Dispatched(EditorActionOutcome::AuthoringConstruction(receipt)) =
            &finished[count].2
        else {
            panic!("construction lost its exact receipt")
        };
        assert_eq!(receipt.entity(&local.unwrap()), allocated.as_ref());
        assert!(receipt.report().applied);
    }
    let stale = host
        .prepare(&application, |_, context| {
            context
                .commit_authoring_edit(retained.take().unwrap())
                .unwrap();
        })
        .unwrap();
    host.submit_and_drive(&mut application, stale, Duration::ZERO);
    assert!(
        matches!(&evidence.finished.borrow()[count + 1].2, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if !report.applied)
    );
    assert_eq!(caption(&application, document), INITIAL);
    assert_eq!(
        current(&application)
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    assert!(application.close());
}
