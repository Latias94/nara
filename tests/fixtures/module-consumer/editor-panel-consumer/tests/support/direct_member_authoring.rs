//! An independently compiled panel discovers and edits a direct durable Record member.

use super::*;

use nara::{
    reflect::{ComponentPropertyStep, ComponentShapeMember, ComponentValueShape},
    scene::AuthoringRecordReadiness,
};
use nara_editor::workspace::EditorPanelTarget;
use nara_tooling::{EditorAuthoringCursor, EditorAuthoringMemberStatus, EditorAuthoringPage};

const COMPONENT: &str = "consumer.direct.PanelSettings";
const PANEL: &str = "consumer.direct-member";
const ORIGINAL: u64 = 41;

fn schemas() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Direct member settings",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("settings"),
                    "Settings",
                    ComponentFieldPath::from_fields(["wire_settings"]),
                    ComponentValueShape::record_members([
                        ComponentShapeMember::new("wire_padding", ComponentValueKind::String)
                            .with_id(ComponentFieldId::new("padding")),
                        ComponentShapeMember::new("wire_score", ComponentValueKind::U64)
                            .with_id(ComponentFieldId::new("score")),
                    ]),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn component_value(score: u64) -> ComponentValue {
    ComponentValue::map([("wire_settings", settings_value(score))])
}

fn settings_value(score: u64) -> ComponentValue {
    ComponentValue::map([
        (
            "wire_padding",
            ComponentValue::String("untouched".repeat(2048)),
        ),
        ("wire_score", ComponentValue::U64(score)),
    ])
}

fn query(
    context: &ImguiEditorPanelContext<'_, '_>,
    position: EditorAuthoringQueryPosition,
) -> EditorAuthoringPage {
    let limits = EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap();
    let page = context
        .query_document_authoring(EditorAuthoringQuery { position, limits })
        .unwrap();
    assert!(page.scanned_entries <= limits.max_scan_entries());
    assert!(page.rows.len() <= limits.max_rows());
    assert!(page.metadata_bytes <= limits.max_metadata_bytes());
    page
}

fn selected_read(target: EditorPropertyTarget) -> EditorPropertyRead {
    let metadata = target.entity.as_str().len()
        + target.component.as_str().len()
        + target.property.logical_bytes().unwrap();
    EditorPropertyRead {
        target,
        limits: EditorPropertyReadLimits::new(metadata, 1, 8)
            .unwrap()
            .with_scan_limits(128, 1024)
            .unwrap(),
    }
}

#[derive(Default)]
struct Evidence {
    observation: Option<EditorPropertyObservation>,
    continuation: Option<EditorAuthoringCursor>,
    held_frames: usize,
    submissions: Vec<PanelSubmissionId>,
    completions: Vec<(PanelSubmissionId, PanelActionDispatch)>,
}

struct DirectMemberPanel {
    document: EditorDocumentId,
    prepared: Option<PreparedEditorDocumentEdit>,
    stale: Option<PreparedEditorDocumentEdit>,
    evidence: Rc<RefCell<Evidence>>,
}

impl ImguiEditorPanel for DirectMemberPanel {
    fn show(&mut self, _: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        assert!(context.model().document().is_none());
        if self.evidence.borrow().observation.is_none() {
            let basis = context.document_query_basis(self.document).unwrap();
            let entities = context
                .query_document_entities(EditorEntityQuery {
                    position: EditorEntityQueryPosition::Start(basis.clone()),
                    limits: EditorEntityQueryLimits::new(2, 1).unwrap(),
                })
                .unwrap();
            assert_eq!(entities.basis, basis);
            assert!(entities.next.is_none());
            let [entity] = entities.entities.as_slice() else {
                panic!("the named non-active Scene must expose its one direct entity");
            };
            let entity = entity.clone();
            let start = |projection| {
                EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: basis.clone(),
                    entity: entity.clone(),
                    projection,
                })
            };
            let mut components = query(context, start(EditorAuthoringProjection::Components));
            let (component, version) = loop {
                assert_eq!(components.target.basis, basis);
                assert_eq!(components.target.entity, entity);
                if let Some(found) = components.rows.iter().find_map(|row| match row {
                    EditorAuthoringRow::Component {
                        component,
                        version,
                        readiness: AuthoringRecordReadiness::Available,
                    } => Some((component.clone(), *version)),
                    _ => None,
                }) {
                    break found;
                }
                components = query(
                    context,
                    EditorAuthoringQueryPosition::Continue(
                        components.next.expect("an available component must follow"),
                    ),
                );
            };
            assert_eq!(component.as_str(), COMPONENT);
            let roots = query(
                context,
                start(EditorAuthoringProjection::Roots {
                    component: component.clone(),
                }),
            );
            assert!(roots.next.is_none());
            let [
                EditorAuthoringRow::Root {
                    field,
                    kind: ComponentValueKind::Map,
                    edit_capability: true,
                    ..
                },
            ] = roots.rows.as_slice()
            else {
                panic!("the actual component must declare an editable Record root");
            };
            let members = query(
                context,
                start(EditorAuthoringProjection::Members {
                    component: component.clone(),
                    root: field.clone(),
                }),
            );
            assert!(matches!(
                members.rows.as_slice(),
                [EditorAuthoringRow::Member {
                    kind: Some(ComponentValueKind::String),
                    status: EditorAuthoringMemberStatus::Available,
                    ..
                }]
            ));
            let continuation = members.next.unwrap();
            let selected = query(
                context,
                EditorAuthoringQueryPosition::Continue(continuation.clone()),
            );
            assert_eq!(selected.target, members.target);
            assert!(selected.next.is_none());
            let [
                EditorAuthoringRow::Member {
                    property,
                    kind: Some(ComponentValueKind::U64),
                    edit_capability: true,
                    status: EditorAuthoringMemberStatus::Available,
                },
            ] = selected.rows.as_slice()
            else {
                panic!("the continuation must discover the direct editable scalar");
            };
            assert_eq!(
                property,
                &ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
                    ComponentPropertyStep::Member(ComponentFieldId::new("score")),
                ])
            );
            let request = selected_read(EditorPropertyTarget {
                basis: selected.target.basis.clone(),
                entity: selected.target.entity.clone(),
                component,
                property: property.clone(),
            });
            let mut whole_root = request.clone();
            whole_root.target.property.steps.clear();
            assert_eq!(
                context.read_document_property(whole_root),
                Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
            );
            let observed = context.read_document_property(request.clone()).unwrap();
            assert_eq!(observed.target, request.target);
            assert_eq!(observed.value_nodes, 1);
            assert_eq!(observed.value_logical_bytes, 8);
            assert_eq!(observed.metadata_bytes, request.limits.max_metadata_bytes());
            assert!(observed.scanned_entries <= request.limits.max_scan_entries());
            assert!(observed.scanned_logical_bytes <= request.limits.max_scan_logical_bytes());
            let EditorPropertyValue::Present(ComponentValue::U64(original)) = observed.value else {
                panic!("the discovered member must supply its actual source scalar");
            };
            assert_eq!(original, ORIGINAL);
            let edits = [AuthoringPropertyEdit {
                entity: observed.target.entity.clone(),
                component: observed.target.component.clone(),
                component_version: version,
                property: observed.target.property.clone(),
                value: ComponentValue::U64(original.checked_add(2).unwrap()),
            }];
            let cancelled = context
                .prepare_authoring_property_edit(&basis, &edits)
                .unwrap();
            assert_eq!(cancelled.document(), self.document);
            assert_eq!(cancelled.base_revision(), basis.revision);
            assert_eq!(cancelled.patch().operations.len(), 1);
            assert_eq!(cancelled.inverse().operations.len(), 1);
            drop(cancelled);
            self.prepared = Some(
                context
                    .prepare_authoring_property_edit(&basis, &edits)
                    .unwrap(),
            );
            self.stale = Some(
                context
                    .prepare_authoring_property_edit(&basis, &edits)
                    .unwrap(),
            );
            let mut evidence = self.evidence.borrow_mut();
            evidence.observation = Some(observed);
            evidence.continuation = Some(continuation);
            return;
        }
        if self.prepared.is_some() {
            let mut evidence = self.evidence.borrow_mut();
            if evidence.held_frames < 2 {
                evidence.held_frames += 1;
                return;
            }
            for prepared in [self.prepared.take().unwrap(), self.stale.take().unwrap()] {
                evidence
                    .submissions
                    .push(context.commit_authoring_edit(prepared).unwrap());
            }
        }
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        let mut evidence = self.evidence.borrow_mut();
        assert!(evidence.submissions.contains(&id));
        assert!(
            !evidence
                .completions
                .iter()
                .any(|(completed, _)| *completed == id)
        );
        evidence.completions.push((id, result));
    }
}

#[test]
fn a_registered_panel_discovers_direct_members_and_round_trips_a_nonactive_scene_edit() {
    let project = text_project::TextProject::new();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schemas().to_json_string().unwrap(),
    )
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    fs::write(project.root.join("scenes/startup.scene.json"), &startup).unwrap();
    let source = "scenes/direct-member.scene.json";
    let contents = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("anchor"))
        .with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, component_value(ORIGINAL))
                .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
        )
        .with_component(
            ComponentTypeId::new("consumer.Unknown"),
            SceneComponentRecord::new(
                ComponentSchemaVersion(7),
                ComponentValue::String("preserve exactly".into()),
            ),
        )])
    .to_json_string()
    .unwrap();
    fs::write(project.root.join(source), &contents).unwrap();
    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |frame| {
        !frame.project_browser().has_pending_work()
    });
    let active = frame(&app).workspace().active_document.unwrap();
    let opening = submit(
        &mut app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: source.into(),
        }),
    );
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Preparing { request },
        )),
    ] = opening.actions()
    else {
        panic!("the direct-member Scene Open must be admitted: {opening:?}");
    };
    let request = *request;
    wait(&mut app, |frame| frame.document_open().result().is_some());
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = frame(&app).document_open().result().unwrap()
    else {
        panic!("the direct-member Scene must open");
    };
    assert_eq!(completed, request);
    let acknowledged = submit(
        &mut app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request: actual }
        ))
    ] if *actual == request));
    for action in [
        EditorNavigationAction::SetActiveDocument { document: active },
        EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(id("active")),
        },
    ] {
        assert!(
            matches!(submit(&mut app, EditorAction::Navigate(action)).actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
        ] if report.applied)
        );
    }
    let initial = frame(&app);
    assert_ne!(document, active);
    let initial_tab = initial.workspace().document(document).unwrap();
    assert_eq!(initial_tab.kind, EditorDocumentKind::Scene);
    assert!(initial_tab.source_binding.is_some());
    assert!(!initial_tab.dirty);
    assert_eq!(initial_tab.history.undo_depth, 0);
    assert_eq!(initial_tab.history.redo_depth, 0);
    let assert_navigation = |app: &EditorApplication| {
        let current = frame(app);
        assert_eq!(current.workspace().active_document, Some(active));
        assert_eq!(
            current.workspace().active_scene,
            initial.workspace().active_scene
        );
        assert_eq!(
            current.scene_preview_document(),
            initial.scene_preview_document()
        );
        assert_eq!(
            current.workspace().documents.len(),
            initial.workspace().documents.len()
        );
        assert_eq!(
            current.workspace().document(active),
            initial.workspace().document(active)
        );
        assert_eq!(
            current
                .workspace()
                .document(document)
                .unwrap()
                .source_binding,
            initial_tab.source_binding
        );
        assert_eq!(
            fs::read_to_string(project.root.join("scenes/startup.scene.json")).unwrap(),
            startup
        );
    };
    let captured = basis(&app, document);
    let evidence = Rc::new(RefCell::new(Evidence::default()));
    let shared = Rc::clone(&evidence);
    let mut composition = EditorNativeComposition::new();
    composition
        .register_panel(
            EditorPanelDescriptor::new(
                PANEL,
                "Direct member",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Floating,
                true,
            ),
            move || DirectMemberPanel {
                document,
                prepared: None,
                stale: None,
                evidence: Rc::clone(&shared),
            },
        )
        .unwrap();
    let mut frontend = composition.install_panels(&mut app).unwrap();
    let panel = app
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == PANEL)
        .unwrap()
        .id();
    assert!(
        app.panel_workspace_mut()
            .layout_mut()
            .set_target(panel, EditorPanelTarget::Workspace)
    );
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
    for _ in 0..8 {
        let draw = context.begin_frame();
        let candidate = frontend.show(draw.ui(), &mut app).unwrap();
        drop(draw);
        assert_eq!(
            frame(&app).workspace().documents,
            initial.workspace().documents,
            "discovery, read failure, preparation, drop, retention and enqueue publish nothing"
        );
        assert!(evidence.borrow().completions.is_empty());
        assert_eq!(
            fs::read_to_string(project.root.join(source)).unwrap(),
            contents
        );
        assert_navigation(&app);
        if let Some(candidate) = candidate {
            frontend.submit_and_drive(&mut app, candidate, Duration::ZERO);
        }
        assert_navigation(&app);
        if evidence.borrow().completions.len() == 2 {
            break;
        }
    }
    let retained = evidence.borrow();
    assert_eq!(retained.held_frames, 2);
    assert_eq!(retained.submissions.len(), 2);
    assert_ne!(retained.submissions[0], retained.submissions[1]);
    let [
        (accepted_id, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(accepted))),
        (stale_id, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(stale))),
    ] = retained.completions.as_slice()
    else {
        panic!("the registered panel must receive both exact consuming outcomes");
    };
    assert_eq!(*accepted_id, retained.submissions[0]);
    assert_eq!(*stale_id, retained.submissions[1]);
    assert!(accepted.applied, "{:?}", accepted.diagnostics);
    assert_eq!(accepted.document, Some(document));
    assert_eq!(accepted.active_document, Some(active));
    assert!(!stale.applied);
    assert_eq!(stale.document, Some(document));
    assert!(stale.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "tooling.workspace-stale-document-revision"
    }));
    let observation = retained.observation.clone().unwrap();
    assert_eq!(observation.target.basis, captured);
    let stale_continuation = EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Continue(retained.continuation.clone().unwrap()),
        limits: EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap(),
    };
    drop(retained);
    drop(context);
    drop(frontend);

    let edited = frame(&app);
    let tab = edited.workspace().document(document).unwrap();
    assert!(tab.dirty);
    assert_eq!(tab.history.undo_depth, 1);
    assert_eq!(tab.history.redo_depth, 0);
    assert_eq!(tab.revision.source_id(), captured.revision.source_id());
    let rejected = submit(
        &mut app,
        EditorAction::QueryDocumentAuthoring(stale_continuation.clone()),
    );
    assert!(matches!(
        rejected.actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::DocumentAuthoring(Err(
                EditorDocumentQueryRejection::StaleRevision
            ))
        )]
    ));
    let old_read = selected_read(observation.target.clone());
    assert_eq!(
        root_read(&mut app, old_read.clone()),
        Err(EditorDocumentQueryRejection::StaleRevision)
    );
    assert_eq!(
        frame(&app).workspace().documents,
        edited.workspace().documents
    );
    assert_eq!(
        fs::read_to_string(project.root.join(source)).unwrap(),
        contents
    );
    let assert_value = |app: &mut EditorApplication, expected| {
        let mut target = observation.target.clone();
        target.basis = basis(app, document);
        assert_eq!(
            root_read(app, selected_read(target.clone())).unwrap().value,
            EditorPropertyValue::Present(ComponentValue::U64(expected))
        );
        target.property.steps.clear();
        let root = EditorPropertyRead {
            target,
            limits: EditorPropertyReadLimits::new(1024, 4, 32768)
                .unwrap()
                .with_scan_limits(128, 32768)
                .unwrap(),
        };
        assert_eq!(
            root_read(app, root).unwrap().value,
            EditorPropertyValue::Present(settings_value(expected)),
            "the complete known root, including its untouched sibling, must survive history"
        );
    };
    assert_value(&mut app, ORIGINAL + 2);
    for (redo, value) in [(false, ORIGINAL), (true, ORIGINAL + 2)] {
        let base_revision = basis(&app, document).revision;
        let mutation = if redo {
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
            matches!(submit(&mut app, EditorAction::Mutate(mutation)).actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
        ] if report.applied && report.document == Some(document))
        );
        assert_value(&mut app, value);
        let current = frame(&app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(tab.dirty, redo);
        assert_eq!(tab.history.undo_depth, usize::from(redo));
        assert_eq!(tab.history.redo_depth, usize::from(!redo));
        assert_navigation(&app);
        assert_eq!(
            fs::read_to_string(project.root.join(source)).unwrap(),
            contents
        );
    }
    let saving = basis(&app, document);
    assert!(matches!(
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: saving.revision,
            })
        )
        .actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        )]
    ));
    settle_persistence(&mut app);
    assert!(
        matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Saved {
        document: saved, revision, ..
    }) if saved == document && revision == saving.revision)
    );
    assert!(!frame(&app).workspace().document(document).unwrap().dirty);
    let saved = fs::read(project.root.join(source)).unwrap();
    let mut expected: serde_json::Value = serde_json::from_str(&contents).unwrap();
    expected["payload"]["entities"][0]["components"][COMPONENT]["value"] =
        serde_json::to_value(component_value(ORIGINAL + 2)).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&saved).unwrap(),
        expected,
        "only the discovered direct member changes; padding and the unknown record survive"
    );
    assert_navigation(&app);
    let attempt = frame(&app).persistence().attempt().unwrap();
    submit(
        &mut app,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    let undo_revision = basis(&app, document).revision;
    assert!(matches!(
        submit(
            &mut app,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: undo_revision,
            })
        )
        .actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
            if report.applied && report.document == Some(document)
    ));
    assert_value(&mut app, ORIGINAL);
    assert!(frame(&app).workspace().document(document).unwrap().dirty);
    assert_eq!(fs::read(project.root.join(source)).unwrap(), saved);
    let reopening = basis(&app, document);
    assert!(matches!(
        submit(
            &mut app,
            EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: reopening.revision,
                },
            })
        )
        .actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        )]
    ));
    settle_persistence(&mut app);
    assert!(
        matches!(frame(&app).persistence().result(), Some(EditorPersistenceResult::Opened {
        document: opened, ..
    }) if opened == document)
    );
    assert_ne!(
        basis(&app, document).revision.source_id(),
        saving.revision.source_id()
    );
    let reopened = frame(&app);
    let tab = reopened.workspace().document(document).unwrap();
    assert!(!tab.dirty);
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 0);
    let rejected = submit(
        &mut app,
        EditorAction::QueryDocumentAuthoring(stale_continuation),
    );
    assert!(matches!(
        rejected.actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::DocumentAuthoring(Err(
                EditorDocumentQueryRejection::StaleIncarnation
            ))
        )]
    ));
    assert_eq!(
        root_read(&mut app, old_read),
        Err(EditorDocumentQueryRejection::StaleIncarnation)
    );
    assert_eq!(
        frame(&app).workspace().documents,
        reopened.workspace().documents
    );
    assert_value(&mut app, ORIGINAL + 2);
    assert_eq!(fs::read(project.root.join(source)).unwrap(), saved);
    assert_navigation(&app);
    assert!(
        app.close(),
        "the direct-member project did not physically retire"
    );
}
