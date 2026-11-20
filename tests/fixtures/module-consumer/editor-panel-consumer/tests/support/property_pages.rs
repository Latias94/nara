//! A registered panel computes a durable edit from bounded pages in a non-active source.

use super::*;

use nara::{
    reflect::{ComponentFieldPathSegment, ComponentShapeMember, ComponentValueShape},
    scene::AuthoringRecordReadiness,
};
use nara_editor::{EditorPanel, EditorPanelContext, EditorPanelHost, workspace::EditorPanelTarget};
use nara_tooling::{
    EditorPersistenceView, EditorPropertyPageData, EditorPropertyPageItem,
    EditorPropertyPagePosition, EditorPropertyPageRead, EditorPropertyPageValue,
    MAX_EDITOR_PROPERTY_READ_LOGICAL_BYTES, MAX_EDITOR_PROPERTY_READ_VALUE_NODES,
};

const PANEL: &str = "consumer.property-pages";
const COMPONENT: &str = "consumer.pages.PanelSettings";
const STARTUP: &str = "scenes/startup.scene.json";
const SAMPLE_COUNT: usize = 4_097;
const ORIGINAL_SCORE: u64 = 7;
const UPDATED_SCORE: u64 = 42;

fn schemas() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(COMPONENT),
                    "Paged settings",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("samples"),
                        "Samples",
                        ComponentFieldPath::from_fields(["wire_samples"]),
                        ComponentValueShape::list(ComponentValueKind::U64.into()),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("score"),
                        "Score",
                        ComponentFieldPath::from_fields(["wire_score"]),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                    ComponentFieldSchema::optional_with_default(
                        ComponentFieldId::new("bonuses"),
                        "Bonuses",
                        ComponentFieldPath::from_fields(["wire_bonuses"]),
                        ComponentValueShape::list(ComponentValueShape::record_members([
                            ComponentShapeMember::new("wire_bonus", ComponentValueKind::U64)
                                .with_id(ComponentFieldId::new("bonus")),
                        ])),
                        ComponentValue::List(vec![ComponentValue::map([(
                            "wire_bonus",
                            ComponentValue::U64(3),
                        )])]),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn component_value(score: u64) -> ComponentValue {
    ComponentValue::map([
        (
            "wire_samples",
            ComponentValue::List((19..).take(SAMPLE_COUNT).map(ComponentValue::U64).collect()),
        ),
        ("wire_score", ComponentValue::U64(score)),
    ])
}

#[derive(Default)]
struct PropertyPagesPanel {
    completion: Option<(PanelSubmissionId, PanelActionDispatch)>,
}

impl EditorPanel for PropertyPagesPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        assert!(self.completion.replace((id, result)).is_none());
    }
}

fn prepare_from_pages(
    context: &EditorPanelContext<'_, '_>,
    document: EditorDocumentId,
) -> (
    EditorPropertyTarget,
    EditorPropertyTarget,
    EditorPropertyPageRead,
    PreparedEditorDocumentEdit,
) {
    assert!(context.model().document().is_none());
    assert_ne!(
        context.model().frame().workspace().active_document,
        Some(document)
    );
    let captured = context.document_query_basis(document).unwrap();
    let entities = context
        .query_document_entities(EditorEntityQuery {
            position: EditorEntityQueryPosition::Start(captured.clone()),
            limits: EditorEntityQueryLimits::new(2, 1).unwrap(),
        })
        .unwrap();
    assert_eq!(entities.basis, captured);
    assert!(entities.next.is_none());
    let [entity] = entities.entities.as_slice() else {
        panic!("the named document must expose its direct entity");
    };
    let query = |projection| {
        let page = context
            .query_document_authoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis: captured.clone(),
                    entity: entity.clone(),
                    projection,
                }),
                limits: EditorAuthoringQueryLimits::new(8, 8, 1024).unwrap(),
            })
            .unwrap();
        assert_eq!(page.target.basis, captured);
        assert_eq!(&page.target.entity, entity);
        assert!(page.next.is_none());
        page
    };
    let components = query(EditorAuthoringProjection::Components);
    let [
        EditorAuthoringRow::Component {
            component,
            version,
            readiness: AuthoringRecordReadiness::Available,
        },
    ] = components.rows.as_slice()
    else {
        panic!("expected one readable component: {:?}", components.rows);
    };
    assert_eq!(component.as_str(), COMPONENT);
    let roots = query(EditorAuthoringProjection::Roots {
        component: component.clone(),
    });
    let target = |name: &str, expected_kind| {
        let field = roots
            .rows
            .iter()
            .find_map(|row| match row {
                EditorAuthoringRow::Root {
                    field,
                    kind,
                    edit_capability: true,
                    ..
                } if field.as_str() == name && *kind == expected_kind => Some(field.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected the declared root {name}"));
        EditorPropertyTarget {
            basis: roots.target.basis.clone(),
            entity: roots.target.entity.clone(),
            component: component.clone(),
            property: ComponentPropertyPath::new(field),
        }
    };
    let samples = target("samples", ComponentValueKind::List);
    let whole = EditorPropertyReadLimits::new(
        1024,
        MAX_EDITOR_PROPERTY_READ_VALUE_NODES,
        MAX_EDITOR_PROPERTY_READ_LOGICAL_BYTES,
    )
    .unwrap();
    let limits = EditorPropertyReadLimits::new(1024, 2, 8)
        .unwrap()
        .with_scan_limits(32, 1024)
        .unwrap();
    assert_eq!(
        context.read_document_property(EditorPropertyRead {
            target: samples.clone(),
            limits: whole,
        }),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    let read_page = |position| {
        let page = context
            .read_document_property_page(EditorPropertyPageRead { position, limits })
            .unwrap();
        assert!(page.metadata_bytes <= limits.max_metadata_bytes());
        assert!(page.value_nodes <= limits.max_value_nodes());
        assert!(page.value_logical_bytes <= limits.max_logical_bytes());
        assert!(page.scanned_entries <= limits.max_scan_entries());
        assert!(page.scanned_logical_bytes <= limits.max_scan_logical_bytes());
        page
    };
    let mut position = EditorPropertyPagePosition::Start {
        target: samples.clone(),
        path: ComponentFieldPath::empty(),
    };
    let mut sum = 0;
    for (offset, expected) in [19, 20].into_iter().enumerate() {
        let page = read_page(position);
        assert_eq!(page.target(), &samples);
        assert!(page.path().is_empty());
        let EditorPropertyPageValue::Present(EditorPropertyPageData::List {
            total_items,
            start,
            items,
        }) = page.value
        else {
            panic!("the source must return a page of its stored samples");
        };
        assert_eq!(total_items, SAMPLE_COUNT);
        assert_eq!(start, offset);
        let [EditorPropertyPageItem::Scalar(ComponentValue::U64(sample))] = items.as_slice() else {
            panic!("the output ceiling allows exactly one sample: {items:?}");
        };
        assert_eq!(*sample, expected);
        sum += *sample;
        position = EditorPropertyPagePosition::Continue(page.next.unwrap());
    }

    let bonuses = target("bonuses", ComponentValueKind::List);
    let path = ComponentFieldPath::new([
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("wire_bonus"),
    ]);
    let page = read_page(EditorPropertyPagePosition::Start {
        target: bonuses.clone(),
        path: path.clone(),
    });
    assert_eq!(page.target(), &bonuses);
    assert_eq!(page.path(), &path);
    assert!(page.next.is_none());
    let EditorPropertyPageValue::Defaulted(EditorPropertyPageData::Scalar(ComponentValue::U64(
        bonus,
    ))) = page.value
    else {
        panic!("the raw position must resolve inside the immutable schema default");
    };
    assert_eq!(bonus, 3);

    let score = target("score", ComponentValueKind::U64);
    let prepared = context
        .prepare_authoring_property_edit(
            &score.basis,
            &[AuthoringPropertyEdit {
                entity: score.entity.clone(),
                component: score.component.clone(),
                component_version: *version,
                property: score.property.clone(),
                value: ComponentValue::U64(sum + bonus),
            }],
        )
        .unwrap();
    let set_score = |value| ScenePatchOperation::SetField {
        entity: score.entity.clone(),
        component: score.component.clone(),
        component_version: *version,
        field: score.property.root.clone(),
        value: ComponentValue::U64(value),
    };
    assert_eq!(prepared.document(), document);
    assert_eq!(prepared.base_revision(), captured.revision);
    assert_eq!(
        prepared.patch().operations.as_slice(),
        &[set_score(UPDATED_SCORE)]
    );
    assert_eq!(
        prepared.inverse().operations.as_slice(),
        &[set_score(ORIGINAL_SCORE)]
    );
    (
        score,
        samples,
        EditorPropertyPageRead { position, limits },
        prepared,
    )
}

fn persist(
    app: &mut EditorApplication,
    action: EditorPersistenceAction,
) -> EditorPersistenceResult {
    let submission = submit(app, EditorAction::Persistence(action));
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { attempt },
        )),
    ] = submission.actions()
    else {
        panic!("the persistence request must be accepted: {submission:?}");
    };
    settle_persistence(app);
    let EditorPersistenceView::Terminal {
        attempt: completed,
        result,
    } = frame(app).persistence()
    else {
        panic!("the accepted persistence attempt must publish a terminal result");
    };
    assert_eq!(completed, *attempt);
    let acknowledge = EditorPersistenceAction::AcknowledgeResult { attempt: completed };
    let acknowledged = submit(app, EditorAction::Persistence(acknowledge));
    assert!(matches!(acknowledged.actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt },
        ))] if *attempt == completed
    ));
    result
}

#[test]
fn a_registered_panel_pages_values_and_round_trips_a_nonactive_scene_edit() {
    round_trip(EditorDocumentKind::Scene, "scenes/value-pages.scene.json");
}

#[test]
fn a_registered_panel_pages_values_and_round_trips_a_nonactive_prefab_edit() {
    round_trip(
        EditorDocumentKind::Prefab,
        "prefabs/value-pages.prefab.json",
    );
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schemas().to_json_string().unwrap(),
    )
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    let startup_path = project.root.join(STARTUP);
    fs::write(&startup_path, &startup).unwrap();
    let entities = [SceneEntityRecord::new(id("anchor")).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, component_value(ORIGINAL_SCORE))
            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    )];
    let original = match kind {
        EditorDocumentKind::Scene => SceneDocument::from_ordered_roots(entities).to_json_string(),
        EditorDocumentKind::Prefab => PrefabDocument::from_ordered_roots(entities).to_json_string(),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap();
    let source_path = project.root.join(source);
    fs::write(&source_path, &original).unwrap();

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
        panic!("Browser Open must admit the named source: {opening:?}");
    };
    wait(&mut app, |frame| frame.document_open().result().is_some());
    let open_result = frame(&app).document_open().result();
    let Some(EditorDocumentOpenResult::Opened {
        request: opened,
        document,
    }) = open_result
    else {
        panic!("the named source must open: {open_result:?}");
    };
    assert_eq!(opened, *request);
    assert_ne!(document, active);
    let acknowledged = submit(
        &mut app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
            request: opened,
        }),
    );
    assert!(matches!(acknowledged.actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request },
        ))] if *request == opened
    ));
    for action in [
        EditorNavigationAction::SetActiveDocument { document: active },
        EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(id("active")),
        },
    ] {
        assert!(matches!(
            submit(&mut app, EditorAction::Navigate(action)).actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
                if report.applied
        ));
    }
    let initial = frame(&app);
    let initial_scene = initial.workspace().active_scene.as_ref().unwrap();
    assert_eq!(initial_scene.document, active);
    assert_eq!(initial_scene.selection.top_entity(), Some(&id("active")));
    assert_eq!(initial.scene_preview_document(), Some(active));
    let initial_tab = initial.workspace().document(document).unwrap();
    assert_eq!(initial_tab.kind, kind);
    assert!(initial_tab.source_binding.is_some());
    assert!(!initial_tab.dirty);
    let assert_navigation = |app: &EditorApplication| {
        let current = frame(app);
        let workspace = current.workspace();
        assert_eq!(workspace.active_document, Some(active));
        assert_eq!(workspace.active_scene.as_ref(), Some(initial_scene));
        assert_eq!(current.scene_preview_document(), Some(active));
        assert_eq!(
            workspace.documents.len(),
            initial.workspace().documents.len()
        );
        assert_eq!(
            workspace.document(active),
            initial.workspace().document(active)
        );
        let tab = workspace.document(document).unwrap();
        assert_eq!(tab.source_binding, initial_tab.source_binding);
        assert_eq!(
            tab.authoring_schema_basis,
            initial_tab.authoring_schema_basis
        );
    };

    let mut host = EditorPanelHost::new();
    host.register_panel(
        &mut app,
        EditorPanelDescriptor::new(
            PANEL,
            "Property value pages",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Floating,
            false,
        ),
        PropertyPagesPanel::default,
    )
    .unwrap();
    let instance = app
        .panel_workspace_mut()
        .open(PANEL, EditorPanelTarget::Workspace)
        .unwrap();
    let mut observation = None;
    let submission = host
        .prepare(&app, |panel, context| {
            assert!(panel.completion.is_none());
            let token = context.instance_token();
            assert_eq!(token.instance(), instance);
            let (score, samples, continuation, prepared) = prepare_from_pages(context, document);
            assert_eq!(prepared.document_kind(), kind);
            let submitted = context.commit_authoring_edit(prepared).unwrap();
            assert!(
                observation
                    .replace((token, submitted, score, samples, continuation))
                    .is_none()
            );
        })
        .unwrap();
    let (token, submitted, score, mut samples, continuation) = observation.unwrap();
    assert_eq!(score.basis, basis(&app, document));
    assert_eq!(submission.submissions().len(), 1);
    assert_eq!(submission.submissions().next(), Some((instance, submitted)));
    assert_eq!(
        frame(&app).workspace().documents,
        initial.workspace().documents
    );
    assert_navigation(&app);
    assert_eq!(fs::read_to_string(&source_path).unwrap(), original);
    assert_eq!(fs::read_to_string(&startup_path).unwrap(), startup);

    host.submit_and_drive(&mut app, submission, Duration::ZERO);
    let edited = frame(&app);
    let tab = edited.workspace().document(document).unwrap();
    host.inspect(&app, &token, |panel, _| {
        let Some((
            finished,
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
        )) = &panel.completion
        else {
            panic!(
                "the panel must receive its exact Commit result: {:?}",
                panel.completion
            );
        };
        assert_eq!(*finished, submitted);
        assert!(report.applied, "{:?}", report.diagnostics);
        assert_eq!(report.document, Some(document));
        assert_eq!(report.active_document, Some(active));
        assert_eq!(report.revision, Some(tab.revision));
    })
    .unwrap();
    assert_ne!(tab.revision, score.basis.revision);
    assert_eq!(tab.revision.source_id(), score.basis.revision.source_id());
    assert_eq!(tab.saved_revision, initial_tab.saved_revision);
    assert!(tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (1, 0));
    assert_navigation(&app);
    assert_eq!(fs::read_to_string(&source_path).unwrap(), original);

    let list_limits = continuation.limits;
    let stale = submit(
        &mut app,
        EditorAction::ReadDocumentPropertyPage(continuation),
    );
    assert!(matches!(
        stale.actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::DocumentPropertyPage(Err(
                EditorDocumentQueryRejection::StaleRevision
            ),)
        )]
    ));
    assert_eq!(
        frame(&app).workspace().documents,
        edited.workspace().documents
    );

    let score_request = |target| EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Start {
            target,
            path: ComponentFieldPath::empty(),
        },
        limits: EditorPropertyReadLimits::new(1024, 1, 8).unwrap(),
    };
    let assert_score = |app: &mut EditorApplication, expected| {
        let target = EditorPropertyTarget {
            basis: basis(app, document),
            ..score.clone()
        };
        let observed = submit(
            app,
            EditorAction::ReadDocumentPropertyPage(score_request(target.clone())),
        );
        let [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentPropertyPage(
                Ok(page),
            )),
        ] = observed.actions()
        else {
            panic!("the current score must be readable as a value page: {observed:?}");
        };
        assert_eq!(page.target(), &target);
        assert!(page.next.is_none());
        assert_eq!(
            page.value,
            EditorPropertyPageValue::Present(EditorPropertyPageData::Scalar(ComponentValue::U64(
                expected,
            )))
        );
    };
    assert_score(&mut app, UPDATED_SCORE);

    let current_read = score_request(EditorPropertyTarget {
        basis: basis(&app, document),
        ..score.clone()
    });
    let closing = submit(
        &mut app,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
            EditorWorkspaceIntent::CloseDocument { document },
        )),
    );
    assert!(
        matches!(
            closing.actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted,)
            )]
        ),
        "{closing:?}"
    );
    wait(&mut app, |frame| {
        frame.workspace_intent().decision_prompt().is_some()
    });
    let pending = frame(&app);
    let prompt = pending.workspace_intent().decision_prompt().unwrap();
    assert_eq!(pending.workspace().documents, edited.workspace().documents);
    assert_eq!(
        app.read_document_property_page(current_read.clone()),
        Err(EditorDocumentQueryRejection::Closing)
    );
    let filtered = submit(
        &mut app,
        EditorAction::ReadDocumentPropertyPage(current_read),
    );
    assert!(
        matches!(
            filtered.actions(),
            [EditorApplicationActionDispatch::NotDispatched(
                EditorApplicationActionNotDispatched::Closing,
            )]
        ),
        "{filtered:?}"
    );
    let cancelled = submit(
        &mut app,
        EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Cancel)),
    );
    assert!(
        matches!(
            cancelled.actions(),
            [EditorApplicationActionDispatch::Dispatched(
                EditorActionOutcome::WorkspaceIntent(_)
            )]
        ),
        "{cancelled:?}"
    );
    let resumed = frame(&app);
    assert!(resumed.workspace_intent().decision_prompt().is_none());
    assert_eq!(resumed.workspace().documents, edited.workspace().documents);
    submit(
        &mut app,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
    );
    assert_navigation(&app);
    assert_eq!(fs::read_to_string(&source_path).unwrap(), original);

    for (redo, expected) in [(false, ORIGINAL_SCORE), (true, UPDATED_SCORE)] {
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
        assert!(matches!(
            submit(&mut app, EditorAction::Mutate(action)).actions(),
            [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
                if report.applied && report.document == Some(document)
        ));
        assert_score(&mut app, expected);
        let current = frame(&app);
        let tab = current.workspace().document(document).unwrap();
        assert_eq!(tab.dirty, redo);
        assert_eq!(
            (tab.history.undo_depth, tab.history.redo_depth),
            (usize::from(redo), usize::from(!redo))
        );
        assert_navigation(&app);
        assert_eq!(fs::read_to_string(&source_path).unwrap(), original);
    }

    let saving = basis(&app, document);
    samples.basis = saving.clone();
    let first = app
        .read_document_property_page(EditorPropertyPageRead {
            position: EditorPropertyPagePosition::Start {
                target: samples.clone(),
                path: ComponentFieldPath::empty(),
            },
            limits: list_limits,
        })
        .unwrap();
    assert_eq!(first.target(), &samples);
    let live_read = EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Continue(first.next.unwrap()),
        limits: list_limits,
    };
    let before_save = app.read_document_property_page(live_read.clone()).unwrap();
    let saved = persist(
        &mut app,
        EditorPersistenceAction::Save {
            document,
            base_revision: saving.revision,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision,
        digest,
    } = saved
    else {
        panic!("the edited source must save: {saved:?}");
    };
    assert_eq!(saved_document, document);
    assert_eq!(revision, saving.revision);
    let saved_frame = frame(&app);
    let tab = saved_frame.workspace().document(document).unwrap();
    assert_eq!(tab.saved_revision, saving.revision);
    assert!(!tab.dirty);
    assert_eq!(
        app.read_document_property_page(live_read.clone()).unwrap(),
        before_save
    );
    let mut expected: serde_json::Value = serde_json::from_str(&original).unwrap();
    expected["payload"]["entities"][0]["components"][COMPONENT]["value"] =
        serde_json::to_value(component_value(UPDATED_SCORE)).unwrap();
    let saved_bytes = fs::read(&source_path).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&saved_bytes).unwrap(),
        expected
    );
    assert_navigation(&app);

    let opened = persist(
        &mut app,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: saving.revision,
            },
        },
    );
    let reopened = frame(&app);
    let tab = reopened.workspace().document(document).unwrap();
    assert_eq!(
        opened,
        EditorPersistenceResult::Opened {
            document,
            revision: tab.revision,
            digest,
        }
    );
    assert_ne!(tab.revision.source_id(), saving.revision.source_id());
    assert_eq!(tab.saved_revision, tab.revision);
    assert!(!tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (0, 0));
    assert_eq!(
        app.read_document_property_page(live_read),
        Err(EditorDocumentQueryRejection::StaleIncarnation)
    );
    assert_score(&mut app, UPDATED_SCORE);
    assert_navigation(&app);
    assert_eq!(fs::read(&source_path).unwrap(), saved_bytes);
    assert_eq!(fs::read_to_string(startup_path).unwrap(), startup);
    assert!(
        app.close(),
        "the property pages project did not physically retire"
    );
}
