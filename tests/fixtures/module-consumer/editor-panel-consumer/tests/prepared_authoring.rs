#![cfg(any(target_os = "windows", target_os = "linux"))]

use std::{
    cell::RefCell,
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentPropertyPath,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueKind, EntityReference, PersistentComponentSchemaOwnerId,
    },
    scene::{
        AuthoringPropertyEdit, PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId,
        SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
};
use nara_editor::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationActionNotDispatched,
    EditorApplicationSubmission, EditorLaunchOptions, EditorNativeComposition, ImguiEditorPanel,
    ImguiEditorPanelContext, PanelActionDispatch, PanelSubmissionId,
    workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorAuthoringPage,
    EditorAuthoringProjection, EditorAuthoringQuery, EditorAuthoringQueryLimits,
    EditorAuthoringQueryPosition, EditorAuthoringQueryTarget, EditorAuthoringRow,
    EditorCloseDecision, EditorDocumentId, EditorDocumentKind, EditorDocumentMutation,
    EditorDocumentOpenResult, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorEntityPage, EditorEntityQuery, EditorEntityQueryLimits, EditorEntityQueryPosition,
    EditorFrameModel, EditorNavigationAction, EditorPersistenceAction, EditorPersistenceOperation,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorProjectBrowserAction,
    EditorProjectBrowserActionOutcome, EditorReopenTarget, EditorRootObservation, EditorRootRead,
    EditorRootReadLimits, EditorRootTarget, EditorRootValue, EditorWorkspaceIntent,
    EditorWorkspaceIntentAction, EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
};

#[path = "support/text_project.rs"]
#[allow(dead_code)] // Shared fixture also supports Check/Play workflows outside this consumer.
mod text_project;

const OWNER: &str = "consumer.prepared";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

struct Application(EditorApplication);
impl Deref for Application {
    type Target = EditorApplication;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Application {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Application {
    fn close(&mut self) -> bool {
        if self.request_exit().exit_ready() {
            return true;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let exit_ready = if let Some(frame) = self.current_frame() {
                let resolution = frame.workspace_intent().decision_prompt().map(|prompt| {
                    EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
                });
                self.submit_and_drive(
                    EditorActionBatch::new(frame.revision(), resolution),
                    Duration::ZERO,
                )
                .update()
                .exit_ready()
            } else {
                self.advance_project_open().exit_ready()
            };
            if exit_ready {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }
}
impl Drop for Application {
    fn drop(&mut self) {
        // Unwind cleanup is best effort; the successful test path asserts physical completion.
        let _ = self.close();
    }
}

fn frame(app: &EditorApplication) -> std::sync::Arc<EditorFrameModel> {
    app.current_frame().unwrap()
}
fn submit(app: &mut EditorApplication, action: EditorAction) -> EditorApplicationSubmission {
    app.submit_and_drive(
        EditorActionBatch::new(frame(app).revision(), [action]),
        Duration::ZERO,
    )
}
fn wait(app: &mut EditorApplication, ready: impl Fn(&EditorFrameModel) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "the public Editor workflow did not settle"
        );
        if let Some(current) = app.current_frame() {
            if ready(&current) {
                return;
            }
            app.submit_and_drive(
                EditorActionBatch::new(current.revision(), []),
                Duration::ZERO,
            );
        } else {
            app.advance_project_open();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn settle_persistence(app: &mut EditorApplication) {
    wait(app, |frame| {
        frame.persistence().operation() == EditorPersistenceOperation::Idle
    });
}

fn bundle() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("consumer.Link"),
                    "Link",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("target"),
                        "Target",
                        ComponentFieldPath::from_fields(["target"]),
                        ComponentValueKind::EntityRef,
                    )
                    .with_capabilities([
                        ComponentCapability::Scene,
                        ComponentCapability::Inspect,
                        ComponentCapability::Edit,
                        ComponentCapability::EntityRef,
                    ]),
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("amount"),
                        "Amount",
                        ComponentFieldPath::from_fields(["amount"]),
                        ComponentValueKind::U64,
                    )
                    .with_numeric_domain(Some(ComponentNumericDomain::U8))
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ])
                .with_authoring_initial_value(ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id("anchor"),
                        }),
                    ),
                    ("amount", ComponentValue::U64(1)),
                ])),
                ComponentSchema::new(
                    ComponentTypeId::new("consumer.Tag"),
                    "Tag",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("value"),
                    "Value",
                    ComponentFieldPath::from_fields(["value"]),
                    ComponentValueKind::String,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                .with_authoring_initial_value(ComponentValue::map([(
                    "value",
                    ComponentValue::String("new".into()),
                )])),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn construction(bundle: &AuthoringSchemaBundle, last_amount: u64) -> ScenePatchDocument {
    let mut operations = vec![];
    for entity in ["created-first", "created-second"] {
        operations.push(ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id(entity)),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        });
    }
    for entity in ["created-first", "created-second"] {
        for schema in &bundle.owners()[0].current().components {
            operations.push(ScenePatchOperation::AddComponent {
                entity: id(entity),
                component: schema.id().clone(),
                value: SceneComponentRecord::new(
                    schema.version(),
                    schema.authoring_initial_value().unwrap().clone(),
                )
                .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            });
        }
    }
    for (entity, target, amount) in [
        ("created-first", "created-second", 7),
        ("created-second", "created-first", last_amount),
    ] {
        operations.push(ScenePatchOperation::SetField {
            entity: id(entity),
            component: ComponentTypeId::new("consumer.Link"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("target"),
            value: ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id(target),
            }),
        });
        operations.push(ScenePatchOperation::SetField {
            entity: id(entity),
            component: ComponentTypeId::new("consumer.Link"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("amount"),
            value: ComponentValue::U64(amount),
        });
    }
    ScenePatchDocument::new(operations)
}

fn basis(app: &EditorApplication, document: EditorDocumentId) -> EditorDocumentQueryBasis {
    let current = frame(app);
    let tab = current.workspace().document(document).unwrap();
    EditorDocumentQueryBasis {
        document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    }
}
fn page(
    app: &mut EditorApplication,
    position: EditorEntityQueryPosition,
) -> Result<EditorEntityPage, EditorDocumentQueryRejection> {
    let submitted = submit(
        app,
        EditorAction::QueryDocumentEntities(EditorEntityQuery {
            position,
            limits: EditorEntityQueryLimits::new(2, 1).unwrap(),
        }),
    );
    match &submitted.actions()[0] {
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(
            result,
        )) => result.clone(),
        other => panic!("query lost its owned result: {other:?}"),
    }
}

fn authoring_page(
    app: &mut EditorApplication,
    position: EditorAuthoringQueryPosition,
) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
    let result = submit(
        app,
        EditorAction::QueryDocumentAuthoring(EditorAuthoringQuery {
            position,
            limits: EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap(),
        }),
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentAuthoring(page))] =
        result.actions()
    else {
        panic!("{result:?}")
    };
    page.clone()
}

fn root_read(
    app: &mut EditorApplication,
    request: EditorRootRead,
) -> Result<EditorRootObservation, EditorDocumentQueryRejection> {
    let result = submit(app, EditorAction::ReadDocumentRoot(request));
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentRoot(value))] =
        result.actions()
    else {
        panic!("{result:?}")
    };
    value.clone()
}

fn exercise_discovered_computed_edit(
    app: &mut EditorApplication,
    document: EditorDocumentId,
    entity: SceneEntityId,
) {
    let captured = basis(app, document);
    let mut components = authoring_page(
        app,
        EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
            basis: captured.clone(),
            entity: entity.clone(),
            projection: EditorAuthoringProjection::Components,
        }),
    )
    .unwrap();
    let mut selected = None;
    loop {
        for row in &components.rows {
            let EditorAuthoringRow::Component {
                component,
                version,
                readiness: nara::scene::AuthoringRecordReadiness::Available,
            } = row
            else {
                continue;
            };
            let mut fields = authoring_page(
                app,
                EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: captured.clone(),
                    entity: entity.clone(),
                    projection: EditorAuthoringProjection::Roots {
                        component: component.clone(),
                    },
                }),
            )
            .unwrap();
            loop {
                for row in &fields.rows {
                    if let EditorAuthoringRow::Root {
                        field,
                        kind: ComponentValueKind::U64,
                        edit_capability: true,
                        ..
                    } = row
                    {
                        selected = Some((component.clone(), *version, field.clone()));
                    }
                }
                if selected.is_some() || fields.next.is_none() {
                    break;
                }
                fields = authoring_page(
                    app,
                    EditorAuthoringQueryPosition::Continue(fields.next.unwrap()),
                )
                .unwrap();
            }
            if selected.is_some() {
                break;
            }
        }
        if selected.is_some() || components.next.is_none() {
            break;
        }
        components = authoring_page(
            app,
            EditorAuthoringQueryPosition::Continue(components.next.unwrap()),
        )
        .unwrap();
    }
    let (component, component_version, field) =
        selected.expect("discover a declared editable integer without knowing its field ID");
    let request = |basis| EditorRootRead {
        target: EditorRootTarget {
            basis,
            entity: entity.clone(),
            component: component.clone(),
            field: field.clone(),
        },
        limits: EditorRootReadLimits::new(1024, 1, 8).unwrap(),
    };
    let old_request = request(captured.clone());
    let observed = root_read(app, old_request.clone()).unwrap();
    let EditorRootValue::Present(ComponentValue::U64(original)) = observed.value else {
        panic!("expected the discovered integer")
    };
    let expected = original.checked_add(2).unwrap();
    let prepared = app
        .prepare_authoring_property_edit(
            document,
            observed.target.basis.revision,
            &[AuthoringPropertyEdit {
                entity: observed.target.entity,
                component: observed.target.component,
                component_version,
                property: ComponentPropertyPath::new(observed.target.field),
                value: ComponentValue::U64(expected),
            }],
        )
        .unwrap();
    let committed = app.commit_authoring_edit(prepared, Duration::ZERO);
    assert!(
        matches!(committed.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
    );
    assert_eq!(
        frame(app)
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    assert_eq!(
        root_read(app, old_request.clone()),
        Err(EditorDocumentQueryRejection::StaleRevision)
    );
    for (redo, value) in [(false, original), (true, expected)] {
        let base_revision = basis(app, document).revision;
        let action = if redo {
            EditorDocumentMutation::Redo {
                document,
                base_revision,
            }
        } else {
            EditorDocumentMutation::Undo {
                document,
                base_revision,
            }
        };
        assert!(
            matches!(submit(app, EditorAction::Mutate(action)).actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        );
        let current_basis = basis(app, document);
        assert_eq!(
            root_read(app, request(current_basis)).unwrap().value,
            EditorRootValue::Present(ComponentValue::U64(value))
        );
    }
    submit(
        app,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
    );
    let saving_basis = basis(app, document);
    assert!(matches!(
        submit(
            app,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: saving_basis.revision
            })
        )
        .actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
        )]
    ));
    settle_persistence(app);
    assert!(
        matches!(frame(app).persistence().result(), Some(EditorPersistenceResult::Saved { document: saved, revision, .. }) if saved == document && revision == saving_basis.revision)
    );
    assert!(!frame(app).workspace().document(document).unwrap().dirty);
    submit(
        app,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
    );
    assert!(matches!(
        submit(
            app,
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: saving_basis.revision
                }
            })
        )
        .actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
        )]
    ));
    settle_persistence(app);
    let reopened = basis(app, document);
    assert_ne!(
        reopened.revision.source_id(),
        saving_basis.revision.source_id()
    );
    assert!(
        matches!(frame(app).persistence().result(), Some(EditorPersistenceResult::Opened { document: opened, .. }) if opened == document)
    );
    assert_eq!(
        root_read(app, request(reopened)).unwrap().value,
        EditorRootValue::Present(ComponentValue::U64(expected))
    );
    assert_eq!(
        root_read(app, old_request),
        Err(EditorDocumentQueryRejection::StaleIncarnation)
    );
}

struct QueryPanel {
    actions: Vec<EditorAction>,
    result: Rc<RefCell<Vec<PanelActionDispatch>>>,
}
impl ImguiEditorPanel for QueryPanel {
    fn show(&mut self, _: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        for action in std::mem::take(&mut self.actions) {
            context.submit(action).unwrap();
        }
    }
    fn action_finished(&mut self, _: PanelSubmissionId, result: PanelActionDispatch) {
        self.result.borrow_mut().push(result);
    }
}

#[test]
fn an_independent_tool_edits_two_scenes_and_two_prefabs_without_selection_choreography() {
    let project = text_project::TextProject::new();
    let schemas = bundle();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schemas.to_json_string().unwrap(),
    )
    .unwrap();
    let records = [
        SceneEntityRecord::new(id("anchor")).with_component(
            ComponentTypeId::new("consumer.Unknown"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve me".into()),
            ),
        ),
        SceneEntityRecord::new(id("tail")),
    ];
    let sources = [
        "scenes/startup.scene.json",
        "scenes/secondary.scene.json",
        "prefabs/primary.prefab.json",
        "prefabs/secondary.prefab.json",
    ];
    for (source, contents) in sources.iter().zip([
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
            .to_json_string()
            .unwrap(),
        SceneDocument::from_ordered_roots(records.clone())
            .to_json_string()
            .unwrap(),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
            .to_json_string()
            .unwrap(),
        PrefabDocument::from_ordered_roots(records)
            .to_json_string()
            .unwrap(),
    ]) {
        fs::write(project.root.join(source), contents).unwrap();
    }
    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |frame| {
        !frame.project_browser().has_pending_work()
    });
    let startup = frame(&app).workspace().active_document.unwrap();
    let mut documents = vec![startup];
    for source in &sources[1..] {
        let before = frame(&app);
        let opening = submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                relative_path: (*source).into(),
            }),
        );
        let [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::Preparing { request },
            )),
        ] = opening.actions()
        else {
            panic!("{opening:?}");
        };
        let request = *request;
        wait(&mut app, |frame| frame.document_open().result().is_some());
        let opened = frame(&app);
        let EditorDocumentOpenResult::Opened {
            request: completed,
            document,
        } = opened.document_open().result().unwrap()
        else {
            panic!("{opened:?}");
        };
        assert_eq!(completed, request);
        assert!(!documents.contains(&document));
        assert_eq!(opened.workspace().documents.len(), documents.len() + 1);
        assert_eq!(opened.workspace().active_document, Some(document));
        assert!(opened.workspace_intent().intent().is_none());
        assert!(opened.workspace_intent().decision_prompt().is_none());
        assert!(opened.workspace_intent().result().is_none());
        for retained in &before.workspace().documents {
            let actual = opened.workspace().document(retained.document).unwrap();
            let mut expected = retained.clone();
            expected.active = actual.active;
            assert_eq!(actual, &expected, "Open must not replace a retained source");
        }
        documents.push(document);
        let acknowledged = submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
        );
        assert!(
            matches!(acknowledged.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged { request: acknowledged }))] if *acknowledged == request),
            "{acknowledged:?}"
        );
        assert!(frame(&app).document_open().result().is_none());
    }
    let [scene, second_scene, prefab, second_prefab] = documents.as_slice() else {
        panic!("the Browser must retain two Scenes and two Prefabs");
    };
    for (document, kind, source) in [
        (*scene, EditorDocumentKind::Scene, sources[0]),
        (*second_scene, EditorDocumentKind::Scene, sources[1]),
        (*prefab, EditorDocumentKind::Prefab, sources[2]),
        (*second_prefab, EditorDocumentKind::Prefab, sources[3]),
    ] {
        let current = frame(&app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(tab.kind, kind);
        assert!(
            tab.source_binding.is_some(),
            "missing source binding for {source}"
        );
    }
    for (document, active, source, panel_kind) in [
        (*second_scene, *scene, sources[1], "consumer.query-scene"),
        (*second_prefab, *prefab, sources[3], "consumer.query-prefab"),
    ] {
        let activated = submit(
            &mut app,
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
        );
        assert!(
            matches!(activated.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied),
            "{activated:?}"
        );
        submit(
            &mut app,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: active,
                entity: Some(id("active")),
            }),
        );
        assert_eq!(
            frame(&app).workspace().active_document,
            Some(active),
            "explicit navigation must settle before the non-active workflow"
        );
        let before = frame(&app);
        let tabs = before.workspace().documents.clone();
        let selection = before
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
            .clone();
        let preview = before.scene_preview_document();
        let other_files: Vec<_> = sources
            .iter()
            .filter(|candidate| **candidate != source)
            .map(|candidate| (*candidate, fs::read(project.root.join(candidate)).unwrap()))
            .collect();
        let assert_untargeted = |app: &EditorApplication| {
            let current = frame(app);
            assert_eq!(current.workspace().documents.len(), 4);
            assert_eq!(current.workspace().active_document, Some(active));
            assert_eq!(current.scene_preview_document(), preview);
            assert_eq!(
                current
                    .workspace()
                    .document(document)
                    .unwrap()
                    .source_binding,
                before
                    .workspace()
                    .document(document)
                    .unwrap()
                    .source_binding
            );
            assert_eq!(
                current.workspace().active_scene.as_ref().unwrap().selection,
                selection
            );
            for retained in tabs.iter().filter(|tab| tab.document != document) {
                assert_eq!(
                    current.workspace().document(retained.document),
                    Some(retained)
                );
            }
            for (path, bytes) in &other_files {
                assert_eq!(fs::read(project.root.join(path)).unwrap(), *bytes);
            }
        };
        let query_basis = basis(&app, document);
        let first = page(
            &mut app,
            EditorEntityQueryPosition::Start(query_basis.clone()),
        )
        .unwrap();
        assert_eq!(first.entities, [id("anchor")]);
        assert_eq!(first.scanned_records, 1);
        let cursor = first.next.unwrap();
        let second = page(
            &mut app,
            EditorEntityQueryPosition::Continue(cursor.clone()),
        )
        .unwrap();
        assert_eq!(second.entities, [id("tail")]);
        assert!(second.next.is_none());
        assert_eq!(
            page(
                &mut app,
                EditorEntityQueryPosition::Continue(cursor.clone())
            )
            .unwrap(),
            second
        );
        assert_eq!(frame(&app).workspace().documents, tabs);
        assert_untargeted(&app);

        let evidence = Rc::new(RefCell::new(Vec::new()));
        let shared = Rc::clone(&evidence);
        let captured = query_basis.clone();
        let mut composition = EditorNativeComposition::new();
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    panel_kind,
                    "Query",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Floating,
                    true,
                ),
                move || QueryPanel {
                    actions: vec![
                        EditorAction::QueryDocumentEntities(EditorEntityQuery {
                            position: EditorEntityQueryPosition::Start(captured.clone()),
                            limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
                        }),
                        EditorAction::QueryDocumentAuthoring(EditorAuthoringQuery {
                            position: EditorAuthoringQueryPosition::Start(
                                EditorAuthoringQueryTarget {
                                    basis: captured.clone(),
                                    entity: id("anchor"),
                                    projection: EditorAuthoringProjection::Components,
                                },
                            ),
                            limits: EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap(),
                        }),
                        EditorAction::ReadDocumentRoot(EditorRootRead {
                            target: EditorRootTarget {
                                basis: captured.clone(),
                                entity: id("anchor"),
                                component: ComponentTypeId::new("consumer.Unknown"),
                                field: ComponentFieldId::new("must-not-interpret"),
                            },
                            limits: EditorRootReadLimits::new(1024, 1, 8).unwrap(),
                        }),
                    ],
                    result: Rc::clone(&shared),
                },
            )
            .unwrap();
        let mut frontend = composition.install_panels(&mut app).unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context.io_mut().set_display_size([1200.0, 800.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        for _ in 0..3 {
            let draw = context.begin_frame();
            let candidate = frontend.show(draw.ui(), &mut app).unwrap();
            drop(draw);
            if let Some(candidate) = candidate {
                frontend.submit_and_drive(&mut app, candidate, Duration::ZERO);
            }
        }
        assert!(matches!(evidence.borrow().as_slice(), [
                PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(result))),
                PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentAuthoring(Ok(actual))),
                PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentRoot(Err(EditorDocumentQueryRejection::Source(nara::scene::AuthoringReadError::UnavailableComponent { readiness: nara::scene::AuthoringRecordReadiness::UnknownSchema })))),
            ] if result.entities == [id("anchor")] && matches!(actual.rows.as_slice(), [EditorAuthoringRow::Component { component, readiness: nara::scene::AuthoringRecordReadiness::UnknownSchema, .. }] if component.as_str() == "consumer.Unknown")));
        assert_eq!(frame(&app).workspace().documents, tabs);

        assert_untargeted(&app);

        let patch = construction(&schemas, 9);
        assert!(
            app.prepare_authoring_edit(
                document,
                query_basis.revision,
                &construction(&schemas, 256)
            )
            .is_err()
        );
        assert_eq!(frame(&app).workspace().documents, tabs);
        let cancelled = app
            .prepare_authoring_edit(document, query_basis.revision, &patch)
            .unwrap();
        assert_eq!(cancelled.patch(), &patch);
        assert_eq!(cancelled.inverse().operations.len(), 10);
        drop(cancelled);
        assert_eq!(frame(&app).workspace().documents, tabs);
        assert_untargeted(&app);
        let prepared = app
            .prepare_authoring_edit(document, query_basis.revision, &patch)
            .unwrap();
        let stale = app
            .prepare_authoring_edit(document, query_basis.revision, &patch)
            .unwrap();
        let result = app.commit_authoring_edit(prepared, Duration::ZERO);
        assert!(
            matches!(result.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        );
        let committed = frame(&app);
        assert_eq!(
            committed
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            1
        );
        assert_untargeted(&app);
        assert_eq!(
            page(&mut app, EditorEntityQueryPosition::Continue(cursor)),
            Err(EditorDocumentQueryRejection::StaleRevision)
        );
        let result = app.commit_authoring_edit(stale, Duration::ZERO);
        assert!(
            matches!(result.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if !report.applied)
        );
        assert_eq!(
            frame(&app).workspace().document(document),
            committed.workspace().document(document)
        );
        assert_untargeted(&app);
        for redo in [false, true] {
            let revision = basis(&app, document).revision;
            let action = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision: revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision: revision,
                }
            };
            let result = submit(&mut app, EditorAction::Mutate(action));
            assert!(
                matches!(result.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
            );
            assert_eq!(
                frame(&app).workspace().document(document).unwrap().dirty,
                redo
            );
            assert_untargeted(&app);
        }
        let revision = basis(&app, document).revision;
        let saving = submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            }),
        );
        assert!(matches!(
            saving.actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
            )]
        ));
        settle_persistence(&mut app);
        assert!(
            matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Saved { document: saved, revision: saved_revision, .. }) if saved == document && saved_revision == revision),
            "{:?}",
            frame(&app).persistence()
        );
        assert!(!frame(&app).workspace().document(document).unwrap().dirty);
        assert_untargeted(&app);
        let bytes = fs::read(project.root.join(source)).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let records = saved["payload"]["entities"].as_array().unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(
            records[0]["components"]["consumer.Unknown"]["value"],
            serde_json::to_value(ComponentValue::String("preserve me".into())).unwrap()
        );
        for (entity, target, amount) in [
            ("created-first", "created-second", 7),
            ("created-second", "created-first", 9),
        ] {
            let record = records
                .iter()
                .find(|record| record["id"] == entity)
                .unwrap();
            assert_eq!(record["components"].as_object().unwrap().len(), 2);
            assert_eq!(
                record["components"]["consumer.Link"]["value"],
                serde_json::to_value(ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: id(target)
                        })
                    ),
                    ("amount", ComponentValue::U64(amount)),
                ]))
                .unwrap()
            );
        }
        let revision = basis(&app, document).revision;
        let old = app
            .prepare_authoring_edit(
                document,
                revision,
                &ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(id("must-not-publish")),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            )
            .unwrap();
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        );
        let reopening = submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: revision,
                },
            }),
        );
        assert!(matches!(
            reopening.actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
            )]
        ));
        settle_persistence(&mut app);
        let reopened_revision = basis(&app, document).revision;
        assert!(
            matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Opened { document: opened, revision: opened_revision, .. }) if opened == document && opened_revision == reopened_revision),
            "{:?}",
            frame(&app).persistence()
        );
        assert_ne!(reopened_revision.source_id(), revision.source_id());
        assert_eq!(fs::read(project.root.join(source)).unwrap(), bytes);
        assert_untargeted(&app);
        let result = app.commit_authoring_edit(old, Duration::ZERO);
        assert!(
            matches!(result.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if !report.applied)
        );
        let mut entities = vec![];
        let mut position = EditorEntityQueryPosition::Start(basis(&app, document));
        for page_index in 0..4 {
            let result = page(&mut app, position).unwrap();
            assert!(result.scanned_records <= 1);
            entities.extend(result.entities);
            let Some(next) = result.next else {
                break;
            };
            assert!(page_index < 3, "four records must finish within four pages");
            position = EditorEntityQueryPosition::Continue(next);
        }
        assert_eq!(
            entities,
            [
                id("anchor"),
                id("created-first"),
                id("created-second"),
                id("tail")
            ]
        );
        exercise_discovered_computed_edit(&mut app, document, entities[1].clone());
        assert!(!frame(&app).workspace().document(document).unwrap().dirty);
        assert_untargeted(&app);
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        );
    }
    for document in [*second_scene, *second_prefab] {
        let before = frame(&app);
        assert_ne!(before.workspace().active_document, Some(document));
        let files: Vec<_> = sources
            .iter()
            .map(|source| fs::read(project.root.join(source)).unwrap())
            .collect();
        let intent = EditorWorkspaceIntent::CloseDocument { document };
        let closing = submit(
            &mut app,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(intent)),
        );
        assert!(matches!(
            closing.actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted)
            )]
        ));
        wait(&mut app, |frame| {
            frame.workspace_intent().result().is_some()
        });
        let closed = frame(&app);
        assert_eq!(
            closed.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Applied { intent })
        );
        assert!(closed.workspace().document(document).is_none());
        assert_eq!(
            closed.workspace().documents.len(),
            before.workspace().documents.len() - 1
        );
        assert_eq!(
            closed.workspace().active_document,
            before.workspace().active_document
        );
        assert_eq!(
            closed.workspace().active_scene,
            before.workspace().active_scene
        );
        assert_eq!(
            closed.scene_preview_document(),
            before.scene_preview_document()
        );
        for retained in before
            .workspace()
            .documents
            .iter()
            .filter(|tab| tab.document != document)
        {
            assert_eq!(
                closed.workspace().document(retained.document),
                Some(retained)
            );
        }
        for (source, bytes) in sources.iter().zip(files) {
            assert_eq!(fs::read(project.root.join(source)).unwrap(), bytes);
        }
        submit(
            &mut app,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
        );
    }
    for (document, source) in [(*scene, sources[0]), (*prefab, sources[2])] {
        let before = frame(&app);
        let other_files: Vec<_> = sources
            .iter()
            .filter(|candidate| **candidate != source)
            .map(|candidate| (*candidate, fs::read(project.root.join(candidate)).unwrap()))
            .collect();
        let revision = basis(&app, document).revision;
        let prepared = app
            .prepare_authoring_edit(
                document,
                revision,
                &ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(id("survivor-saved")),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            )
            .unwrap();
        let committed = app.commit_authoring_edit(prepared, Duration::ZERO);
        assert!(
            matches!(committed.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        );
        let revision = basis(&app, document).revision;
        let saving = submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            }),
        );
        assert!(matches!(
            saving.actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
            )]
        ));
        settle_persistence(&mut app);
        assert!(
            matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Saved { document: saved, revision: saved_revision, .. }) if saved == document && saved_revision == revision),
            "the surviving document lost its file Host: {:?}",
            frame(&app).persistence()
        );
        let current = frame(&app);
        assert_eq!(current.workspace().documents.len(), 2);
        assert_eq!(
            current.workspace().active_document,
            before.workspace().active_document
        );
        assert_eq!(
            current.scene_preview_document(),
            before.scene_preview_document()
        );
        assert_eq!(
            current.workspace().active_scene.as_ref().unwrap().selection,
            before.workspace().active_scene.as_ref().unwrap().selection
        );
        assert!(!current.workspace().document(document).unwrap().dirty);
        for retained in before
            .workspace()
            .documents
            .iter()
            .filter(|tab| tab.document != document)
        {
            assert_eq!(
                current.workspace().document(retained.document),
                Some(retained)
            );
        }
        let bytes = fs::read(project.root.join(source)).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entities = saved["payload"]["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0]["id"], "active");
        assert_eq!(entities[1]["id"], "survivor-saved");
        for (path, bytes) in other_files {
            assert_eq!(fs::read(project.root.join(path)).unwrap(), bytes);
        }
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        );
    }
    assert!(
        app.close(),
        "the four-document owner did not physically retire"
    );
}

#[test]
fn retained_construction_publishes_exact_ids_through_non_active_public_documents() {
    use nara::scene::ConstructionPlacement;

    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let project = text_project::TextProject::new();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            bundle().to_json_string().unwrap(),
        )
        .unwrap();
        let original = [SceneEntityRecord::new(id("anchor")).with_component(
            ComponentTypeId::new("consumer.Unknown"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve me".into()),
            ),
        )];
        for (path, contents) in [
            (
                "scenes/startup.scene.json",
                SceneDocument::from_ordered_roots(original.clone())
                    .to_json_string()
                    .unwrap(),
            ),
            (
                "prefabs/primary.prefab.json",
                PrefabDocument::from_ordered_roots(original)
                    .to_json_string()
                    .unwrap(),
            ),
        ] {
            fs::write(project.root.join(path), contents).unwrap();
        }
        let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
            &project.root,
        )));
        app.native_shell_ready();
        wait(&mut app, |frame| {
            !frame.project_browser().has_pending_work()
        });
        let scene = frame(&app).workspace().active_document.unwrap();
        submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                relative_path: "prefabs/primary.prefab.json".into(),
            }),
        );
        wait(&mut app, |frame| frame.document_open().result().is_some());
        let prefab = frame(&app).workspace().active_document.unwrap();
        assert_ne!(scene, prefab);
        let (document, active, source) = if kind == EditorDocumentKind::Scene {
            (scene, prefab, "scenes/startup.scene.json")
        } else {
            (prefab, scene, "prefabs/primary.prefab.json")
        };
        submit(
            &mut app,
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
        );
        for selected in [document, active] {
            submit(
                &mut app,
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: selected,
                    entity: Some(id("anchor")),
                }),
            );
        }
        let before = frame(&app);
        let revision = basis(&app, document).revision;
        let mut invalid = app
            .begin_authoring_construction(document, revision)
            .unwrap();
        invalid
            .add_component(id("anchor"), ComponentTypeId::new("consumer.Link"))
            .unwrap();
        invalid
            .set_property(
                id("anchor"),
                ComponentTypeId::new("consumer.Link"),
                ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                ComponentValue::U64(256),
            )
            .unwrap();
        assert!(
            app.prepare_authoring_construction(document, &invalid)
                .is_err()
        );
        assert_eq!(
            frame(&app).workspace().documents,
            before.workspace().documents
        );
        let mut draft = app
            .begin_authoring_construction(document, revision)
            .unwrap();
        let first = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let second = draft
            .create_entity(ConstructionPlacement::ChildOf((&first).into()))
            .unwrap();
        for local in [&first, &second] {
            for component in ["consumer.Link", "consumer.Tag"] {
                draft
                    .add_component(local, ComponentTypeId::new(component))
                    .unwrap();
            }
        }
        for (entity, target, amount) in [(&first, &second, 7), (&second, &first, 9)] {
            draft
                .set_entity_reference(
                    entity,
                    ComponentTypeId::new("consumer.Link"),
                    ComponentPropertyPath::new(ComponentFieldId::new("target")),
                    target,
                )
                .unwrap();
            draft
                .set_property(
                    entity,
                    ComponentTypeId::new("consumer.Link"),
                    ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                    ComponentValue::U64(amount),
                )
                .unwrap();
        }
        let cancelled = app
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        assert_eq!(cancelled.document(), document);
        assert_eq!(cancelled.document_kind(), kind);
        assert_eq!(cancelled.base_revision(), revision);
        assert_eq!(cancelled.patch().operations.len(), 10);
        assert_eq!(cancelled.inverse().operations.len(), 10);
        drop(cancelled);
        assert_eq!(
            frame(&app).workspace().documents,
            before.workspace().documents
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection,
            before.workspace().active_scene.as_ref().unwrap().selection
        );

        let prepared = app
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        let stale = app
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        let first_id = prepared.entity(&first).unwrap().clone();
        let second_id = prepared.entity(&second).unwrap().clone();
        assert_ne!(first_id, second_id);
        let committed = app.commit_authoring_construction(prepared, Duration::ZERO);
        assert!(matches!(committed.submission().actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied));
        assert_eq!(committed.entity(&first), Some(&first_id));
        assert_eq!(committed.entity(&second), Some(&second_id));
        let accepted = frame(&app);
        assert_eq!(
            accepted
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection,
            before.workspace().active_scene.as_ref().unwrap().selection
        );
        assert_eq!(
            accepted
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            1
        );
        assert_eq!(accepted.workspace().active_document, Some(active));
        assert_eq!(
            accepted.workspace().document(active),
            before.workspace().document(active)
        );
        assert_eq!(
            accepted.scene_preview_document(),
            before.scene_preview_document()
        );
        let rejected = app.commit_authoring_construction(stale, Duration::ZERO);
        assert!(matches!(rejected.submission().actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if !report.applied));
        assert!(rejected.entity(&first).is_none());
        assert_eq!(
            frame(&app).workspace().documents,
            accepted.workspace().documents
        );

        for redo in [false, true] {
            let base_revision = basis(&app, document).revision;
            let action = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                }
            };
            let result = submit(&mut app, EditorAction::Mutate(action));
            assert!(matches!(result.actions(),
                [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied));
            assert_eq!(
                frame(&app).workspace().document(document).unwrap().dirty,
                redo
            );
        }
        let revision = basis(&app, document).revision;
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            }),
        );
        settle_persistence(&mut app);
        assert!(!frame(&app).workspace().document(document).unwrap().dirty);
        let bytes = fs::read(project.root.join(source)).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let records = saved["payload"]["entities"].as_array().unwrap();
        assert_eq!(records.len(), 3);
        let anchor = records
            .iter()
            .find(|record| record["id"] == "anchor")
            .unwrap();
        assert_eq!(
            anchor["components"]["consumer.Unknown"]["value"],
            serde_json::to_value(ComponentValue::String("preserve me".into())).unwrap()
        );
        assert_eq!(
            saved["payload"]["roots"],
            serde_json::json!(["anchor", first_id.as_str()])
        );
        for (entity, target, amount) in [(&first_id, &second_id, 7), (&second_id, &first_id, 9)] {
            let record = records
                .iter()
                .find(|record| record["id"] == entity.as_str())
                .unwrap();
            if entity == &first_id {
                assert_eq!(record["children"], serde_json::json!([second_id.as_str()]));
            }
            assert_eq!(record["components"].as_object().unwrap().len(), 2);
            assert_eq!(
                record["components"]["consumer.Link"]["value"],
                serde_json::to_value(ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: target.clone()
                        })
                    ),
                    ("amount", ComponentValue::U64(amount)),
                ]))
                .unwrap()
            );
        }
        let mut obsolete = app
            .begin_authoring_construction(document, revision)
            .unwrap();
        let obsolete_local = obsolete
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let old = app
            .prepare_authoring_construction(document, &obsolete)
            .unwrap();
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        );
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: revision,
                },
            }),
        );
        settle_persistence(&mut app);
        assert_ne!(
            basis(&app, document).revision.source_id(),
            revision.source_id()
        );
        assert!(
            app.prepare_authoring_construction(document, &obsolete)
                .is_err()
        );
        let rejected = app.commit_authoring_construction(old, Duration::ZERO);
        assert!(rejected.entity(&obsolete_local).is_none());
        let (rejection, mapping) = rejected.into_parts();
        assert!(mapping.is_none());
        assert!(matches!(rejection.actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if !report.applied));
        assert_eq!(fs::read(project.root.join(source)).unwrap(), bytes);
        assert_eq!(
            committed.entity(&first),
            Some(&first_id),
            "owned results outlive Undo and Reopen"
        );
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection,
            before.workspace().active_scene.as_ref().unwrap().selection
        );
        assert!(app.close());
    }
}

#[test]
fn property_preparation_obeys_document_close_project_replacement_and_retirement() {
    use nara::project_host::EditorEditPreparationError;

    for guard in ["document-close", "project-replacement", "retirement"] {
        let project = text_project::TextProject::new();
        let replacement = text_project::TextProject::new();
        let schemas = bundle();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            schemas.to_json_string().unwrap(),
        )
        .unwrap();
        let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("anchor"))
            .with_component(
                ComponentTypeId::new("consumer.Link"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    schemas.owners()[0].current().components[0]
                        .authoring_initial_value()
                        .unwrap()
                        .clone(),
                )
                .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            )])
        .to_json_string()
        .unwrap();
        let source_path = project.root.join("scenes/startup.scene.json");
        fs::write(&source_path, &source).unwrap();
        let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
            &project.root,
        )));
        app.native_shell_ready();
        wait(&mut app, |frame| {
            !frame.project_browser().has_pending_work()
        });
        let document = frame(&app).workspace().active_document.unwrap();
        let mut edits = [AuthoringPropertyEdit {
            entity: id("anchor"),
            component: ComponentTypeId::new("consumer.Link"),
            component_version: ComponentSchemaVersion::ONE,
            property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            value: ComponentValue::U64(2),
        }];
        let before = frame(&app);
        let prepared = app
            .prepare_authoring_property_edit(document, basis(&app, document).revision, &edits)
            .unwrap();
        assert_eq!(prepared.patch().operations.len(), 1);
        assert_eq!(
            frame(&app).workspace().documents,
            before.workspace().documents
        );
        let committed = app.commit_authoring_edit(prepared, Duration::ZERO);
        assert!(
            matches!(committed.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        );
        edits[0].value = ComponentValue::U64(3);
        let revision = basis(&app, document).revision;
        let prepared = app
            .prepare_authoring_property_edit(document, revision, &edits)
            .unwrap();
        let mut draft = app
            .begin_authoring_construction(document, revision)
            .unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        let construction = app
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        let documents = frame(&app).workspace().documents.clone();
        match guard {
            "document-close" => {
                let requested = submit(
                    &mut app,
                    EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                        EditorWorkspaceIntent::CloseDocument { document },
                    )),
                );
                assert!(matches!(
                    requested.actions(),
                    [EditorApplicationActionDispatch::Dispatched(
                        EditorActionOutcome::WorkspaceIntent(
                            EditorWorkspaceIntentRequestResult::Accepted
                        )
                    )]
                ));
            }
            "project-replacement" => {
                assert!(app.request_project_replacement(&replacement.root, None));
            }
            "retirement" => assert!(!app.request_exit().exit_ready()),
            _ => unreachable!(),
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while frame(&app).workspace_intent().decision_prompt().is_none() {
            assert!(
                Instant::now() < deadline,
                "{guard} never requested a decision"
            );
            app.advance_project_open();
            let revision = frame(&app).revision();
            app.submit_and_drive(EditorActionBatch::new(revision, []), Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            matches!(
                app.prepare_authoring_property_edit(document, revision, &edits),
                Err(EditorEditPreparationError::Closing)
            ),
            "{guard} admitted preparation"
        );
        assert!(matches!(
            app.begin_authoring_construction(document, revision),
            Err(EditorEditPreparationError::Closing)
        ));
        assert!(matches!(
            app.prepare_authoring_construction(document, &draft),
            Err(EditorEditPreparationError::Closing)
        ));
        let rejected_construction = app.commit_authoring_construction(construction, Duration::ZERO);
        assert!(rejected_construction.entity(&local).is_none());
        assert!(matches!(
            rejected_construction.submission().actions(),
            [EditorApplicationActionDispatch::NotDispatched(
                EditorApplicationActionNotDispatched::Closing
            )]
        ));
        let rejected = app.commit_authoring_edit(prepared, Duration::ZERO);
        assert!(
            matches!(
                rejected.actions(),
                [EditorApplicationActionDispatch::NotDispatched(
                    EditorApplicationActionNotDispatched::Closing
                )]
            ),
            "{guard} published a prepared edit"
        );
        assert_eq!(frame(&app).workspace().documents, documents);
        assert_eq!(fs::read_to_string(&source_path).unwrap(), source);
        let resolution = frame(&app)
            .workspace_intent()
            .decision_prompt()
            .unwrap()
            .resolve(EditorCloseDecision::Cancel);
        submit(&mut app, EditorAction::WorkspaceIntent(resolution));
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if app
                .prepare_authoring_property_edit(document, revision, &edits)
                .is_ok()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{guard} cancellation did not restore authoring"
            );
            app.advance_project_open();
            let revision = frame(&app).revision();
            app.submit_and_drive(EditorActionBatch::new(revision, []), Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(frame(&app).workspace().documents, documents);
        submit(
            &mut app,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
        );
        let obsolete = app
            .prepare_authoring_construction(document, &draft)
            .unwrap();
        if guard == "retirement" {
            assert!(app.close());
        } else {
            if guard == "document-close" {
                let requested = submit(
                    &mut app,
                    EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                        EditorWorkspaceIntent::CloseDocument { document },
                    )),
                );
                assert!(matches!(
                    requested.actions(),
                    [EditorApplicationActionDispatch::Dispatched(
                        EditorActionOutcome::WorkspaceIntent(
                            EditorWorkspaceIntentRequestResult::Accepted
                        )
                    )]
                ));
            } else {
                assert!(app.request_project_replacement(&replacement.root, None));
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            while frame(&app).workspace_intent().decision_prompt().is_none() {
                assert!(
                    Instant::now() < deadline,
                    "{guard} never requested final disposal"
                );
                app.advance_project_open();
                let revision = frame(&app).revision();
                app.submit_and_drive(EditorActionBatch::new(revision, []), Duration::ZERO);
                std::thread::sleep(Duration::from_millis(1));
            }
            let resolution = frame(&app)
                .workspace_intent()
                .decision_prompt()
                .unwrap()
                .resolve(EditorCloseDecision::Discard);
            submit(&mut app, EditorAction::WorkspaceIntent(resolution));
            loop {
                app.advance_project_open();
                if let Some(current) = app.current_frame() {
                    let retired = if guard == "document-close" {
                        current.workspace().document(document).is_none()
                    } else {
                        current
                            .workspace()
                            .document(document)
                            .is_some_and(|tab| tab.revision.source_id() != revision.source_id())
                    };
                    if retired {
                        break;
                    }
                    app.submit_and_drive(
                        EditorActionBatch::new(current.revision(), []),
                        Duration::ZERO,
                    );
                }
                assert!(Instant::now() < deadline, "{guard} did not complete");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if guard == "project-replacement" {
            assert_eq!(
                frame(&app).workspace().active_document,
                Some(document),
                "the new project's same raw slot must not authorize the old source"
            );
        }
        let retained = app
            .current_frame()
            .map(|current| current.workspace().documents.clone());
        let rejected = app.commit_authoring_construction(obsolete, Duration::ZERO);
        let (submission, mapping) = rejected.into_parts();
        assert!(mapping.is_none());
        assert!(!matches!(submission.actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied));
        assert_eq!(
            app.current_frame()
                .map(|current| current.workspace().documents.clone()),
            retained
        );
        assert_eq!(fs::read_to_string(&source_path).unwrap(), source);
        assert!(app.close(), "{guard} did not physically retire");
    }
}

#[test]
fn browser_prefab_persistence_round_trips_through_a_fresh_anchored_document() {
    use nara_tooling::{
        EditorPersistenceResult, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
    };
    for prefab_directory in ["prefabs", "content/prefabs"] {
        for reopen_source in [false, true] {
            let project = text_project::TextProject::new();
            let manifest_path = project.root.join("nara.toml");
            let manifest = fs::read_to_string(&manifest_path).unwrap();
            assert!(manifest.contains("prefabs = \"prefabs\""));
            fs::write(
                manifest_path,
                manifest.replace(
                    "prefabs = \"prefabs\"",
                    &format!("prefabs = \"{prefab_directory}\""),
                ),
            )
            .unwrap();
            fs::create_dir_all(project.root.join(prefab_directory)).unwrap();
            fs::write(
                project.root.join("schema/authoring.bundle.json"),
                bundle().to_json_string().unwrap(),
            )
            .unwrap();
            let relative_path = format!("{prefab_directory}/shared.prefab.json");
            let prefab_path = project.root.join(&relative_path);
            fs::write(
                &prefab_path,
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("original"))])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            let mut anchor = SceneEntityRecord::new(id("anchor"));
            anchor.prefab = Some(nara::scene::PrefabInstance {
                source: nara::asset::AssetRef::path("shared.prefab.json").unwrap(),
                overrides: Default::default(),
            });
            fs::write(
                project.root.join("scenes/startup.scene.json"),
                SceneDocument::from_ordered_roots([anchor])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
                &project.root,
            )));
            app.native_shell_ready();
            wait(&mut app, |frame| {
                !frame.project_browser().has_pending_work()
            });
            let scene = frame(&app).workspace().active_document.unwrap();
            submit(
                &mut app,
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                    relative_path: relative_path.into(),
                }),
            );
            wait(&mut app, |frame| frame.document_open().result().is_some());
            let EditorDocumentOpenResult::Opened { request, document } =
                frame(&app).document_open().result().unwrap()
            else {
                panic!("Browser Open did not publish a Prefab");
            };
            submit(
                &mut app,
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request,
                }),
            );
            let revision = basis(&app, document).revision;
            let action = if reopen_source {
                fs::write(
                    &prefab_path,
                    PrefabDocument::from_ordered_roots([
                        SceneEntityRecord::new(id("original")),
                        SceneEntityRecord::new(id("published")),
                    ])
                    .to_json_string()
                    .unwrap(),
                )
                .unwrap();
                EditorPersistenceAction::Reopen {
                    target: EditorReopenTarget::OpenDocument {
                        document,
                        base_revision: revision,
                    },
                }
            } else {
                let prepared = app
                    .prepare_authoring_edit(
                        document,
                        revision,
                        &ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                            entity: SceneEntityRecord::new(id("published")),
                            placement: ScenePlacement::last(ScenePlacementOwner::Root),
                        }]),
                    )
                    .unwrap();
                let committed = app.commit_authoring_edit(prepared, Duration::ZERO);
                assert!(
                    matches!(committed.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
                );
                EditorPersistenceAction::Save {
                    document,
                    base_revision: basis(&app, document).revision,
                }
            };
            let persistence = submit(&mut app, EditorAction::Persistence(action));
            assert!(matches!(
                persistence.actions(),
                [EditorApplicationActionDispatch::Dispatched(
                    EditorActionOutcome::Persistence(
                        nara_tooling::EditorPersistenceRequestResult::Accepted
                    )
                )]
            ));
            settle_persistence(&mut app);
            let result = frame(&app).persistence().result();
            assert!(
                if reopen_source {
                    matches!(result, Some(EditorPersistenceResult::Opened { .. }))
                } else {
                    matches!(result, Some(EditorPersistenceResult::Saved { .. }))
                },
                "{result:?}"
            );
            submit(
                &mut app,
                EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
            );
            let closed = submit(
                &mut app,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                    EditorWorkspaceIntent::CloseDocument { document },
                )),
            );
            assert!(matches!(
                closed.actions(),
                [EditorApplicationActionDispatch::Dispatched(
                    EditorActionOutcome::WorkspaceIntent(
                        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
                    )
                )]
            ));
            wait(&mut app, |frame| {
                frame.workspace().document(document).is_none()
            });
            submit(
                &mut app,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
            );
            submit(
                &mut app,
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                    document: scene,
                }),
            );
            submit(
                &mut app,
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene,
                    entity: Some(id("anchor")),
                }),
            );
            let scene_revision = basis(&app, scene).revision;
            let opened = submit(
                &mut app,
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene,
                    base_revision: scene_revision,
                    entity: id("anchor"),
                }),
            );
            let [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
            ] = opened.actions()
            else {
                panic!("{opened:?}");
            };
            assert!(report.applied, "{:?}", report.diagnostics);
            let reopened = report.opened_document.unwrap();
            assert_ne!(reopened, document);
            let query_basis = basis(&app, reopened);
            let first = page(&mut app, EditorEntityQueryPosition::Start(query_basis)).unwrap();
            assert_eq!(first.entities, [id("original")]);
            let next = first
                .next
                .expect("the bounded query must retain its second source record");
            let second = page(&mut app, EditorEntityQueryPosition::Continue(next)).unwrap();
            assert_eq!(second.entities, [id("published")]);
            assert!(second.next.is_none());
            assert_eq!(frame(&app).scene_preview_document(), Some(scene));
            assert!(
                app.close(),
                "the independent publication owners did not retire"
            );
        }
    }
}
