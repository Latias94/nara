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
        ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, EntityReference, PersistentComponentSchemaOwnerId,
    },
    scene::{
        PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
        ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
};
use nara_editor::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationSubmission,
    EditorLaunchOptions, EditorNativeComposition, ImguiEditorPanel, ImguiEditorPanelContext,
    PanelActionDispatch, PanelSubmissionId,
    workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorCloseDecision, EditorDocumentId,
    EditorDocumentKind, EditorDocumentMutation, EditorDocumentOpenResult, EditorEntityPage,
    EditorEntityQuery, EditorEntityQueryBasis, EditorEntityQueryLimits, EditorEntityQueryPosition,
    EditorEntityQueryRejection, EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
    EditorPersistenceOperation, EditorProjectBrowserAction, EditorReopenTarget,
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

fn basis(app: &EditorApplication, document: EditorDocumentId) -> EditorEntityQueryBasis {
    let current = frame(app);
    let tab = current.workspace().document(document).unwrap();
    EditorEntityQueryBasis {
        document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    }
}
fn page(
    app: &mut EditorApplication,
    position: EditorEntityQueryPosition,
) -> Result<EditorEntityPage, EditorEntityQueryRejection> {
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

struct QueryPanel {
    basis: Option<EditorEntityQueryBasis>,
    result: Rc<RefCell<Vec<PanelActionDispatch>>>,
}
impl ImguiEditorPanel for QueryPanel {
    fn show(&mut self, _: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        if let Some(basis) = self.basis.take() {
            context
                .submit(EditorAction::QueryDocumentEntities(EditorEntityQuery {
                    position: EditorEntityQueryPosition::Start(basis),
                    limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
                }))
                .unwrap();
        }
    }
    fn action_finished(&mut self, _: PanelSubmissionId, result: PanelActionDispatch) {
        self.result.borrow_mut().push(result);
    }
}

#[test]
fn an_independent_tool_queries_and_commits_non_active_scene_and_prefab_without_selection_choreography()
 {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
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
        // The current Host owns one Scene lane and one Prefab lane. Exercise each nominal
        // document as the non-active target without pretending same-kind multi-open exists.
        let (scene_records, prefab_records, source) = match kind {
            EditorDocumentKind::Scene => (
                records.to_vec(),
                vec![SceneEntityRecord::new(id("active"))],
                "scenes/startup.scene.json",
            ),
            EditorDocumentKind::Prefab => (
                vec![SceneEntityRecord::new(id("active"))],
                records.to_vec(),
                "prefabs/secondary.prefab.json",
            ),
            _ => unreachable!(),
        };
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots(scene_records)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("prefabs/secondary.prefab.json"),
            PrefabDocument::from_ordered_roots(prefab_records)
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
        let startup = frame(&app).workspace().active_document.unwrap();
        submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                relative_path: "prefabs/secondary.prefab.json".into(),
            }),
        );
        wait(&mut app, |frame| frame.document_open().result().is_some());
        let opened = frame(&app);
        let EditorDocumentOpenResult::Opened { document, .. } =
            opened.document_open().result().unwrap()
        else {
            panic!("{opened:?}")
        };
        let (document, active) = if kind == EditorDocumentKind::Scene {
            (startup, document)
        } else {
            (document, startup)
        };
        submit(
            &mut app,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                request: opened.document_open().result().unwrap().request(),
            }),
        );
        assert_eq!(frame(&app).workspace().documents.len(), 2);
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
        assert_eq!(
            frame(&app).workspace().active_document,
            Some(active),
            "direct query must not navigate"
        );

        let evidence = Rc::new(RefCell::new(Vec::new()));
        let shared = Rc::clone(&evidence);
        let captured = query_basis.clone();
        let mut composition = EditorNativeComposition::new();
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    "consumer.query",
                    "Query",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Floating,
                    true,
                ),
                move || QueryPanel {
                    basis: Some(captured.clone()),
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
        assert!(
            matches!(evidence.borrow().as_slice(), [PanelActionDispatch::Dispatched(EditorActionOutcome::DocumentEntities(Ok(result)))] if result.entities == [id("anchor")])
        );
        assert_eq!(frame(&app).workspace().documents, tabs);

        assert_eq!(
            frame(&app).workspace().active_document,
            Some(active),
            "panel painting/query dispatch must not navigate"
        );

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
        assert_eq!(committed.workspace().active_document, Some(active));
        assert_eq!(
            committed
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection,
            selection
        );
        assert_eq!(
            page(&mut app, EditorEntityQueryPosition::Continue(cursor)),
            Err(EditorEntityQueryRejection::StaleRevision)
        );
        let result = app.commit_authoring_edit(stale, Duration::ZERO);
        assert!(
            matches!(result.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if !report.applied)
        );
        assert_eq!(
            frame(&app).workspace().document(document),
            committed.workspace().document(document)
        );
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
        assert_eq!(frame(&app).workspace().active_document, Some(active));
        assert_eq!(
            frame(&app)
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .selection,
            selection
        );
        let result = app.commit_authoring_edit(old, Duration::ZERO);
        assert!(
            matches!(result.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if !report.applied)
        );
        let mut entities = vec![];
        let mut position = EditorEntityQueryPosition::Start(basis(&app, document));
        loop {
            let result = page(&mut app, position).unwrap();
            entities.extend(result.entities);
            let Some(next) = result.next else {
                break;
            };
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
        assert!(!frame(&app).workspace().document(document).unwrap().dirty);
        assert!(app.close(), "the independent Editor owner did not retire");
    }
}
