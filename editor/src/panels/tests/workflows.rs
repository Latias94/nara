use nara::{
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::{AuthoringPropertyEdit, SceneDocumentCandidate, ScenePlacement, ScenePlacementParent},
    tooling::{
        EditorAuthoringProjection, EditorAuthoringQuery, EditorAuthoringQueryLimits,
        EditorAuthoringQueryPosition, EditorAuthoringQueryTarget, EditorPersistenceAction,
        EditorPersistenceResult, EditorPersistenceView, EditorProjectBrowserAction,
        EditorPropertyRead, EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    },
};

use crate::panels::submission::{MAX_PANEL_SUBMISSIONS, PanelSubmitError};

use super::*;

fn through(
    host: &mut EditorPanelHost<Probe>,
    application: &mut EditorApplication,
    action: EditorAction,
) {
    let mut action = Some(action);
    let submission = host
        .prepare(application, |_, context| {
            context.submit(action.take().unwrap()).unwrap();
        })
        .unwrap();
    assert!(action.is_none());
    host.submit_and_drive(application, submission, Duration::ZERO);
}

fn settle_persistence(application: &mut EditorApplication) -> EditorPersistenceResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let EditorPersistenceView::Terminal { result, .. } =
            application.current_frame().unwrap().persistence()
        {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "persistence did not reach a terminal"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn open_prefab(application: &mut EditorApplication) -> EditorDocumentId {
    use nara::tooling::{EditorDocumentOpenResult, EditorProjectBrowserActionOutcome};

    let path = "blueprints/external.prefab.json";
    let deadline = Instant::now() + Duration::from_secs(10);
    while application
        .current_frame()
        .unwrap()
        .project_browser()
        .entry(path)
        .is_none()
    {
        assert!(Instant::now() < deadline, "the Prefab was not indexed");
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: path.into(),
                },
            )],
        ),
        Duration::ZERO,
    );
    let [
        crate::EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Preparing { request },
        )),
    ] = submitted.actions()
    else {
        panic!(
            "the indexed Prefab did not start opening: {:?}",
            submitted.actions()
        );
    };
    let request = *request;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = application
            .current_frame()
            .unwrap()
            .document_open()
            .result()
        {
            let EditorDocumentOpenResult::Opened {
                request: completed,
                document,
            } = result
            else {
                panic!("the indexed Prefab did not open: {result:?}");
            };
            assert_eq!(completed, request);
            dispatch(
                application,
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request,
                }),
            );
            return document;
        }
        assert!(
            Instant::now() < deadline,
            "public Browser did not open the Prefab"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn prepared_edit_and_construction_keep_history_receipts_save_and_reopen_in_the_original_owner() {
    let (fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let original = current_basis(&application);
    let source = fixture.project_root().join("worlds/entry.scene.json");
    let original_bytes = fs::read(&source).unwrap();
    let patch = crate::app::named_root_patch(&application, original.document, "prepared-root");
    let edit = host
        .prepare(&application, |_, context| {
            let prepared = context.prepare_authoring_edit(&original, &patch).unwrap();
            assert_eq!(prepared.patch(), &patch);
            context.commit_authoring_edit(prepared).unwrap();
        })
        .unwrap();
    assert_eq!(current_basis(&application), original);
    assert_eq!(fs::read(&source).unwrap(), original_bytes);
    host.submit_and_drive(&mut application, edit, Duration::ZERO);
    assert!(matches!(
        &observed.finished.borrow()[0].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied
    ));
    let after_edit = current_basis(&application);
    assert_ne!(after_edit.revision, original.revision);
    let mut local = None;
    let mut expected = None;
    let construction = host
        .prepare(&application, |_, context| {
            let mut draft = context.begin_authoring_construction(&after_edit).unwrap();
            let entity = draft
                .create_entity(ScenePlacement::last(ScenePlacementParent::Root))
                .unwrap();
            let prepared = context
                .prepare_authoring_construction(&after_edit, &draft)
                .unwrap();
            expected = prepared.entity(&entity).cloned();
            local = Some(entity);
            context.commit_authoring_construction(prepared).unwrap();
        })
        .unwrap();
    assert_eq!(current_basis(&application), after_edit);
    assert_eq!(fs::read(&source).unwrap(), original_bytes);
    host.submit_and_drive(&mut application, construction, Duration::ZERO);
    let created = expected.unwrap();
    {
        let finished = observed.finished.borrow();
        let PanelActionDispatch::Dispatched(EditorActionOutcome::AuthoringConstruction(receipt)) =
            &finished[1].2
        else {
            panic!("construction did not retain its source-owned result");
        };
        assert_eq!(receipt.entity(&local.unwrap()), Some(&created));
        assert!(receipt.report().applied);
    }
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(original.document)
            .unwrap()
            .history
            .undo_depth,
        2
    );
    let current = current_basis(&application);
    through(
        &mut host,
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document: original.document,
            base_revision: current.revision,
        }),
    );
    let undone = current_basis(&application);
    let entities = application
        .query_document_entities(query(&undone, 16))
        .unwrap();
    assert_eq!(entities.entities.len(), 3);
    assert!(!entities.entities.contains(&created));
    through(
        &mut host,
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Redo {
            document: original.document,
            base_revision: undone.revision,
        }),
    );
    let restored = current_basis(&application);
    assert!(
        application
            .query_document_entities(query(&restored, 16))
            .unwrap()
            .entities
            .contains(&created)
    );
    through(
        &mut host,
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document: original.document,
            base_revision: restored.revision,
        }),
    );
    assert!(matches!(
        settle_persistence(&mut application),
        EditorPersistenceResult::Saved { document, .. } if document == original.document
    ));
    let saved_bytes = fs::read(&source).unwrap();
    let saved = SceneDocumentCandidate::decode_json_bytes(&saved_bytes)
        .unwrap()
        .into_composition_source()
        .unwrap();
    assert!(saved.entities.iter().any(|entity| entity.id == created));
    let attempt = application
        .current_frame()
        .unwrap()
        .persistence()
        .attempt()
        .unwrap();
    through(
        &mut host,
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    let target = application.current_frame().unwrap().reopen_target();
    through(
        &mut host,
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
    );
    assert!(matches!(
        settle_persistence(&mut application),
        EditorPersistenceResult::Opened { .. }
    ));
    let reopened = current_basis(&application);
    assert_ne!(reopened.revision, restored.revision);
    assert_eq!(fs::read(&source).unwrap(), saved_bytes);
    let entities = application
        .query_document_entities(query(&reopened, 16))
        .unwrap();
    assert_eq!(entities.entities.len(), 4);
    assert!(entities.entities.contains(&created));
    assert!(
        application
            .query_document_entities(query(&restored, 16))
            .is_err()
    );
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(reopened.document)
            .unwrap()
            .history
            .undo_depth,
        0
    );
    assert_eq!(observed.finished.borrow().len(), 7);
    close(&mut application);
}

#[test]
fn context_queries_and_edits_a_non_active_prefab_without_changing_navigation() {
    let (_fixture, mut application) = application();
    let scene = current_basis(&application);
    let prefab = open_prefab(&mut application);
    dispatch(
        &mut application,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: scene.document,
        }),
    );
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    let instance = register(&mut host, &mut application, &observed, "test.semantic");
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(instance, EditorPanelTarget::Workspace);
    let before = application.current_frame().unwrap();
    let mut basis = None;
    let submission = host
        .prepare(&application, |_, context| {
            assert!(context.model().document().is_none());
            assert!(context.source_tool_review().unwrap().is_none());
            let current = context.document_query_basis(prefab).unwrap();
            let entities = context.query_document_entities(query(&current, 1)).unwrap();
            assert_eq!(
                entities.entities,
                [SceneEntityId::new("external-prefab").unwrap()]
            );
            let entity = entities.entities[0].clone();
            let component = ComponentTypeId::new("nara.scene.Name");
            let page = context
                .query_document_authoring(EditorAuthoringQuery {
                    position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                        basis: current.clone(),
                        entity: entity.clone(),
                        projection: EditorAuthoringProjection::Components,
                    }),
                    limits: EditorAuthoringQueryLimits::new(4, 4, 1024).unwrap(),
                })
                .unwrap();
            assert_eq!(page.rows.len(), 1);
            let property = ComponentPropertyPath::new(ComponentFieldId::new("value"));
            let read = context
                .read_document_property(EditorPropertyRead {
                    target: EditorPropertyTarget {
                        basis: current.clone(),
                        entity: entity.clone(),
                        component: component.clone(),
                        property: property.clone(),
                    },
                    limits: EditorPropertyReadLimits::new(1024, 1, 128).unwrap(),
                })
                .unwrap();
            assert_eq!(
                read.value,
                EditorPropertyValue::Present(ComponentValue::String(
                    "Closure-external Prefab".into()
                ))
            );
            let prepared = context
                .prepare_authoring_property_edit(
                    &current,
                    &[AuthoringPropertyEdit {
                        entity,
                        component,
                        component_version: ComponentSchemaVersion::ONE,
                        property,
                        value: ComponentValue::String("Edited off-tab".into()),
                    }],
                )
                .unwrap();
            context.commit_authoring_edit(prepared).unwrap();
            basis = Some(current);
        })
        .unwrap();
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        before.workspace()
    );
    host.submit_and_drive(&mut application, submission, Duration::ZERO);
    let after = application.current_frame().unwrap();
    assert_eq!(after.workspace().active_document, Some(scene.document));
    assert_eq!(current_basis(&application), scene);
    assert_eq!(
        after.workspace().active_scene,
        before.workspace().active_scene
    );
    let prefab_tab = after.workspace().document(prefab).unwrap();
    assert_eq!(prefab_tab.history.undo_depth, 1);
    assert!(prefab_tab.dirty);
    assert_ne!(prefab_tab.revision, basis.unwrap().revision);
    assert!(matches!(
        &observed.finished.borrow()[0].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied
    ));
    close(&mut application);
}

#[test]
fn capacity_refusal_returns_original_edit_and_construction_for_explicit_retry() {
    let (_fixture, mut application) = application();
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    register(&mut host, &mut application, &observed, "test.semantic");
    let original = current_basis(&application);
    let patch = crate::app::named_root_patch(&application, original.document, "capacity-root");
    let mut retained_edit = None;
    let mut retained_construction = None;
    let mut local = None;
    let mut expected = None;
    let full = host
        .prepare(&application, |_, context| {
            let prepared_edit = context.prepare_authoring_edit(&original, &patch).unwrap();
            let mut draft = context.begin_authoring_construction(&original).unwrap();
            let entity = draft
                .create_entity(ScenePlacement::last(ScenePlacementParent::Root))
                .unwrap();
            let prepared = context
                .prepare_authoring_construction(&original, &draft)
                .unwrap();
            let prepared_patch = prepared.patch().clone();
            let prepared_inverse = prepared.inverse().clone();
            expected = prepared.entity(&entity).cloned();
            for _ in 0..MAX_PANEL_SUBMISSIONS {
                context
                    .submit(EditorAction::QueryDocumentEntities(query(&original, 1)))
                    .unwrap();
            }
            assert_eq!(
                context.submit(EditorAction::QueryDocumentEntities(query(&original, 1))),
                Err(PanelSubmitError::Capacity)
            );
            let (error, returned_edit) = context.commit_authoring_edit(prepared_edit).unwrap_err();
            assert_eq!(error, PanelSubmitError::Capacity);
            assert_eq!(returned_edit.patch(), &patch);
            assert_eq!(returned_edit.base_revision(), original.revision);
            let (error, returned) = context.commit_authoring_construction(prepared).unwrap_err();
            assert_eq!(error, PanelSubmitError::Capacity);
            assert_eq!(returned.patch(), &prepared_patch);
            assert_eq!(returned.inverse(), &prepared_inverse);
            assert_eq!(returned.entity(&entity), expected.as_ref());
            retained_edit = Some(returned_edit);
            retained_construction = Some(returned);
            local = Some(entity);
        })
        .unwrap();
    drop(full);
    assert!(observed.finished.borrow().is_empty());
    assert_eq!(current_basis(&application), original);
    let retry = host
        .prepare(&application, |_, context| {
            context
                .commit_authoring_construction(retained_construction.take().unwrap())
                .unwrap();
        })
        .unwrap();
    assert_eq!(observed.finished.borrow().len(), MAX_PANEL_SUBMISSIONS);
    assert!(observed.finished.borrow().iter().all(|(_, _, result)| {
        *result == PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded)
    }));
    host.submit_and_drive(&mut application, retry, Duration::ZERO);
    {
        let finished = observed.finished.borrow();
        let PanelActionDispatch::Dispatched(EditorActionOutcome::AuthoringConstruction(receipt)) =
            &finished[MAX_PANEL_SUBMISSIONS].2
        else {
            panic!("retry lost its original construction receipt");
        };
        assert_eq!(receipt.entity(&local.unwrap()), expected.as_ref());
    }
    let committed = current_basis(&application);
    let stale = host
        .prepare(&application, |_, context| {
            context
                .commit_authoring_edit(retained_edit.take().unwrap())
                .unwrap();
        })
        .unwrap();
    host.submit_and_drive(&mut application, stale, Duration::ZERO);
    assert_eq!(current_basis(&application), committed);
    assert!(matches!(
        &observed.finished.borrow()[MAX_PANEL_SUBMISSIONS + 1].2,
        PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if !report.applied
    ));
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(original.document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    close(&mut application);
}

#[test]
fn active_document_resolution_and_explicit_target_changes_retire_the_exact_typed_state() {
    let (_fixture, mut application) = application();
    let scene = current_basis(&application);
    let mut host = EditorPanelHost::new();
    let observed = Rc::new(Observations::default());
    let instance = register(&mut host, &mut application, &observed, "test.semantic");
    let old = host
        .prepare(&application, |_, context| {
            context
                .submit(EditorAction::QueryDocumentEntities(query(&scene, 1)))
                .unwrap();
        })
        .unwrap();
    let prefab = open_prefab(&mut application);
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_document,
        Some(prefab)
    );
    host.submit_and_drive(&mut application, old, Duration::ZERO);
    assert_eq!(*observed.dropped.borrow(), [1]);
    assert!(observed.finished.borrow().is_empty());
    let _ = host
        .prepare(&application, |panel, context| {
            assert_eq!(panel.serial, 2);
            assert_eq!(context.model().document().unwrap().document, prefab);
        })
        .unwrap();
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_target(instance, EditorPanelTarget::Workspace);
    let _ = host
        .prepare(&application, |panel, context| {
            assert_eq!(panel.serial, 3);
            assert!(context.model().document().is_none());
        })
        .unwrap();
    dispatch(
        &mut application,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: scene.document,
        }),
    );
    let _ = host
        .prepare(&application, |panel, _| assert_eq!(panel.serial, 3))
        .unwrap();
    assert_eq!(observed.created.get(), 3);
    assert_eq!(*observed.dropped.borrow(), [1, 2]);
    close(&mut application);
}
