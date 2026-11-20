//! Registered panels review and commit edits from bounded reads of non-active sources.

use super::*;

use nara::{
    diagnostic::DiagnosticValueRef,
    project_host::EditorEditPreparationError,
    reflect::{
        ComponentFieldPathSegment, ComponentPropertyError, ComponentPropertyKey,
        ComponentPropertyStep, ComponentShapeMember, ComponentValueShape,
    },
    scene::{AuthoringReadError, AuthoringRecordReadiness},
};
use nara_editor::{
    EditorPanel, EditorPanelContext, EditorPanelHost,
    workspace::{EditorPanelInstanceId, EditorPanelTarget},
};
use nara_tooling::{
    EditorDocumentDigest, EditorPersistenceView, EditorPropertyPage, EditorPropertyPageData,
    EditorPropertyPageItem, EditorPropertyPagePosition, EditorPropertyPageRead,
    EditorPropertyPageValue, EditorWorkspaceCommandReport, MAX_EDITOR_PROPERTY_READ_LOGICAL_BYTES,
    MAX_EDITOR_PROPERTY_READ_VALUE_NODES,
};

#[path = "property_pages/round_eight.rs"]
mod round_eight;

const PANEL: &str = "consumer.property-pages";
const COMPONENT: &str = "consumer.pages.PanelSettings";
const STARTUP: &str = "scenes/startup.scene.json";
const SAMPLE_COUNT: usize = 4_097;
const KEYED_ROW_COUNT: usize = 32_768;
const KEYED_SCAN_ENTRIES: usize = 128;
const ORIGINAL_SCORE: u64 = 7;
const UPDATED_SCORE: u64 = 42;
const FIRST_ROW_KEY: &str = "row-0";
const OLD_ROW_KEY: &str = "chosen/original";
const NEW_ROW_KEY: &str = "chosen/renamed";
const INTERMEDIATE_ROW_SCORE: u64 = 19;
const ORIGINAL_NOTE: &str = "keep this note";
const UPDATED_NOTE: &str = "edit this note";

fn schemas() -> AuthoringSchemaBundle {
    schema_bundle([
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
    ])
}

fn schema_bundle(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> AuthoringSchemaBundle {
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
                .with_fields(fields),
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
    retained: Option<PreparedEditorDocumentEdit>,
}

impl PropertyPagesPanel {
    fn completed(&self, submitted: PanelSubmissionId) -> &EditorWorkspaceCommandReport {
        let Some((
            finished,
            PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
        )) = &self.completion
        else {
            panic!(
                "the panel must receive its exact Commit result: {:?}",
                self.completion
            );
        };
        assert_eq!(*finished, submitted);
        report
    }
}

impl EditorPanel for PropertyPagesPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        assert!(self.completion.replace((id, result)).is_none());
    }
}

fn root_page_request(
    target: EditorPropertyTarget,
    limits: EditorPropertyReadLimits,
) -> EditorPropertyPageRead {
    EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Start {
            target,
            path: ComponentFieldPath::empty(),
        },
        limits,
    }
}

fn assert_page_limits(page: &EditorPropertyPage, limits: EditorPropertyReadLimits) {
    assert!(page.metadata_bytes <= limits.max_metadata_bytes());
    assert!(page.value_nodes <= limits.max_value_nodes());
    assert!(page.value_logical_bytes <= limits.max_logical_bytes());
    assert!(page.scanned_entries <= limits.max_scan_entries());
    assert!(page.scanned_logical_bytes <= limits.max_scan_logical_bytes());
}

fn sample_on_page(page: &EditorPropertyPage, samples: &EditorPropertyTarget, offset: usize) -> u64 {
    assert_eq!(page.target(), samples);
    assert!(page.path().is_empty());
    let EditorPropertyPageValue::Present(EditorPropertyPageData::List {
        total_items,
        start,
        items,
    }) = &page.value
    else {
        panic!("the source must return a page of its stored samples");
    };
    assert_eq!(*total_items, SAMPLE_COUNT);
    assert_eq!(*start, offset);
    let [EditorPropertyPageItem::Scalar(ComponentValue::U64(sample))] = items.as_slice() else {
        panic!("the output ceiling allows exactly one sample: {items:?}");
    };
    *sample
}

fn assert_preview_score(prepared: &PreparedEditorDocumentEdit, request: &EditorPropertyRead) {
    let observed = prepared.preview_property(request.clone()).unwrap();
    assert_eq!(observed.target, request.target);
    assert_eq!(observed.value_nodes, 1);
    assert_eq!(observed.value_logical_bytes, 8);
    let EditorPropertyValue::Present(ComponentValue::U64(score)) = &observed.value else {
        panic!("the prepared candidate must expose its proposed score: {observed:?}");
    };
    assert_eq!(*score, UPDATED_SCORE);

    let paged = root_page_request(observed.target.clone(), request.limits);
    let page = prepared.preview_property_page(paged.clone()).unwrap();
    assert_eq!(page.target(), &observed.target);
    assert!(page.path().is_empty());
    assert!(page.next.is_none());
    assert_page_limits(&page, request.limits);
    assert_eq!(
        page.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::Scalar(ComponentValue::U64(
            *score,
        )))
    );
    assert_eq!(prepared.preview_property(request.clone()), Ok(observed));
    assert_eq!(prepared.preview_property_page(paged), Ok(page));
}

fn prepare_from_pages(
    context: &EditorPanelContext<'_, '_>,
    document: EditorDocumentId,
) -> (
    EditorPropertyRead,
    EditorPropertyTarget,
    EditorPropertyPageRead,
    [PreparedEditorDocumentEdit; 2],
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
    let whole_samples = EditorPropertyRead {
        target: samples.clone(),
        limits: EditorPropertyReadLimits::new(
            1024,
            MAX_EDITOR_PROPERTY_READ_VALUE_NODES,
            MAX_EDITOR_PROPERTY_READ_LOGICAL_BYTES,
        )
        .unwrap(),
    };
    let limits = EditorPropertyReadLimits::new(1024, 2, 8)
        .unwrap()
        .with_scan_limits(32, 1024)
        .unwrap();
    assert_eq!(
        context.read_document_property(whole_samples.clone()),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    let read_page = |position| {
        let page = context
            .read_document_property_page(EditorPropertyPageRead { position, limits })
            .unwrap();
        assert_page_limits(&page, limits);
        page
    };
    let samples_start = root_page_request(samples.clone(), limits);
    let mut position = samples_start.position.clone();
    let mut sum = 0;
    for (offset, expected) in [19, 20].into_iter().enumerate() {
        let page = read_page(position);
        let sample = sample_on_page(&page, &samples, offset);
        assert_eq!(sample, expected);
        sum += sample;
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
    let edits = [AuthoringPropertyEdit {
        entity: score.entity.clone(),
        component: score.component.clone(),
        component_version: *version,
        property: score.property.clone(),
        value: ComponentValue::U64(sum + bonus),
    }];
    let prepare = || {
        context
            .prepare_authoring_property_edit(&score.basis, &edits)
            .unwrap()
    };
    let prepared = prepare();
    let retained = prepare();
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
    assert_eq!(retained.base_revision(), prepared.base_revision());

    let score_read = EditorPropertyRead {
        target: score,
        limits: EditorPropertyReadLimits::new(1024, 1, 8).unwrap(),
    };
    assert_eq!(
        prepared.preview_property(whole_samples),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    let live_continuation = EditorPropertyPageRead { position, limits };
    for (candidate, sibling) in [(&prepared, &retained), (&retained, &prepared)] {
        assert_preview_score(candidate, &score_read);
        let first = candidate
            .preview_property_page(samples_start.clone())
            .unwrap();
        assert_page_limits(&first, limits);
        assert_eq!(sample_on_page(&first, &samples, 0), 19);
        let continuation = EditorPropertyPageRead {
            position: EditorPropertyPagePosition::Continue(first.next.unwrap()),
            limits,
        };
        let next = candidate
            .preview_property_page(continuation.clone())
            .unwrap();
        assert_page_limits(&next, limits);
        assert_eq!(sample_on_page(&next, &samples, 1), 20);
        assert_eq!(
            candidate.preview_property_page(continuation.clone()),
            Ok(next)
        );
        assert_eq!(
            context.read_document_property_page(continuation.clone()),
            Err(EditorDocumentQueryRejection::WrongPreparedEdit)
        );
        assert_eq!(
            sibling.preview_property_page(continuation),
            Err(EditorDocumentQueryRejection::WrongPreparedEdit)
        );
        assert_eq!(
            candidate.preview_property_page(live_continuation.clone()),
            Err(EditorDocumentQueryRejection::WrongPreparedEdit)
        );
    }
    assert_eq!(
        context
            .read_document_property(score_read.clone())
            .unwrap()
            .value,
        EditorPropertyValue::Present(ComponentValue::U64(ORIGINAL_SCORE))
    );
    (score_read, samples, live_continuation, [prepared, retained])
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

fn open_nonactive_source(
    project: &text_project::TextProject,
    kind: EditorDocumentKind,
    source: &str,
    schemas: AuthoringSchemaBundle,
    entities: impl IntoIterator<Item = SceneEntityRecord>,
) -> (Application, EditorDocumentId, String, String) {
    let original = match kind {
        EditorDocumentKind::Scene => SceneDocument::from_ordered_roots(entities).to_json_string(),
        EditorDocumentKind::Prefab => PrefabDocument::from_ordered_roots(entities).to_json_string(),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    .unwrap();
    let (app, document, startup) =
        open_nonactive_source_text(project, kind, source, schemas, &original);
    (app, document, original, startup)
}

fn open_nonactive_source_text(
    project: &text_project::TextProject,
    kind: EditorDocumentKind,
    source: &str,
    schemas: AuthoringSchemaBundle,
    original: &str,
) -> (Application, EditorDocumentId, String) {
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schemas.to_json_string().unwrap(),
    )
    .unwrap();
    let startup = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
        .to_json_string()
        .unwrap();
    fs::write(project.root.join(STARTUP), &startup).unwrap();
    fs::write(project.root.join(source), original).unwrap();

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
    assert_eq!(
        (
            initial_tab.history.undo_depth,
            initial_tab.history.redo_depth
        ),
        (0, 0)
    );
    (app, document, startup)
}

fn assert_navigation(
    app: &EditorApplication,
    initial: &EditorFrameModel,
    document: EditorDocumentId,
) {
    let current = frame(app);
    let workspace = current.workspace();
    let original = initial.workspace();
    let active = original.active_document.unwrap();
    assert_eq!(workspace.active_document, Some(active));
    assert_eq!(workspace.active_scene, original.active_scene);
    assert_eq!(current.scene_preview_document(), Some(active));
    assert_eq!(workspace.documents.len(), original.documents.len());
    assert_eq!(workspace.document(active), original.document(active));
    let tab = workspace.document(document).unwrap();
    let initial_tab = original.document(document).unwrap();
    assert_eq!(tab.source_binding, initial_tab.source_binding);
    assert_eq!(
        tab.authoring_schema_basis,
        initial_tab.authoring_schema_basis
    );
}

fn register_panel(
    app: &mut EditorApplication,
) -> (EditorPanelHost<PropertyPagesPanel>, EditorPanelInstanceId) {
    let mut host = EditorPanelHost::new();
    host.register_panel(
        app,
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
    (host, instance)
}

fn round_trip(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let entities = [SceneEntityRecord::new(id("anchor")).with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, component_value(ORIGINAL_SCORE))
            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
    )];
    let (mut app, document, original, startup) =
        open_nonactive_source(&project, kind, source, schemas(), entities);
    let source_path = project.root.join(source);
    let startup_path = project.root.join(STARTUP);
    let initial = frame(&app);
    let active = initial.workspace().active_document.unwrap();
    let initial_tab = initial.workspace().document(document).unwrap();
    let assert_navigation = |app: &EditorApplication| assert_navigation(app, &initial, document);

    let (mut host, instance) = register_panel(&mut app);
    let mut observation = None;
    let submission = host
        .prepare(&app, |panel, context| {
            assert!(panel.completion.is_none());
            let token = context.instance_token();
            assert_eq!(token.instance(), instance);
            let (score_read, samples, continuation, [prepared, retained]) =
                prepare_from_pages(context, document);
            assert_eq!(prepared.document_kind(), kind);
            assert!(panel.retained.replace(retained).is_none());
            let submitted = context.commit_authoring_edit(prepared).unwrap();
            assert!(
                observation
                    .replace((token, submitted, score_read, samples, continuation))
                    .is_none()
            );
        })
        .unwrap();
    let (token, submitted, score_read, mut samples, continuation) = observation.unwrap();
    let score = &score_read.target;
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
        let report = panel.completed(submitted);
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

    let score_request = |target| root_page_request(target, score_read.limits);
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
        if !redo {
            let live_score = EditorPropertyTarget {
                basis: basis(&app, document),
                ..score.clone()
            };
            let stale_submission = host
                .prepare(&app, |panel, context| {
                    assert_eq!(panel.completion.take().map(|(id, _)| id), Some(submitted));
                    let retained = panel.retained.take().unwrap();
                    assert_preview_score(&retained, &score_read);
                    assert_eq!(
                        retained.preview_property(EditorPropertyRead {
                            target: live_score.clone(),
                            limits: score_read.limits,
                        }),
                        Err(EditorDocumentQueryRejection::StaleRevision)
                    );
                    assert_eq!(
                        retained.preview_property_page(score_request(live_score.clone())),
                        Err(EditorDocumentQueryRejection::StaleRevision)
                    );
                    context.commit_authoring_edit(retained).unwrap();
                })
                .unwrap();
            assert_eq!(stale_submission.submissions().len(), 1);
            let (origin, stale_id) = stale_submission.submissions().next().unwrap();
            assert_eq!(origin, instance);
            assert_ne!(stale_id, submitted);
            host.submit_and_drive(&mut app, stale_submission, Duration::ZERO);
            host.inspect(&app, &token, |panel, _| {
                let report = panel.completed(stale_id);
                assert!(!report.applied);
                assert_eq!(report.document, Some(document));
                assert_eq!(report.active_document, Some(active));
                assert_eq!(report.revision, Some(tab.revision));
                assert!(report.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "tooling.workspace-stale-document-revision"
                }));
            })
            .unwrap();
            assert_score(&mut app, ORIGINAL_SCORE);
            assert_eq!(
                frame(&app).workspace().documents,
                current.workspace().documents
            );
        }
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

fn keyed_rows_schema() -> AuthoringSchemaBundle {
    schema_bundle([
        ComponentFieldSchema::required(
            ComponentFieldId::new("rows"),
            "Rows",
            ComponentFieldPath::from_fields(["wire_rows"]),
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("wire_key", ComponentValueKind::String)
                        .with_id(ComponentFieldId::new("id")),
                    ComponentShapeMember::new("wire_score", ComponentValueKind::U64)
                        .with_id(ComponentFieldId::new("score")),
                ]),
                ComponentFieldId::new("id"),
            ),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["wire_note"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn keyed_rows_entity() -> SceneEntityRecord {
    let row = |key, score| {
        ComponentValue::map([
            ("wire_key", ComponentValue::String(key)),
            ("wire_score", ComponentValue::U64(score)),
        ])
    };
    let rows = (0..KEYED_ROW_COUNT - 1)
        .map(|index| row(format!("row-{index}"), index as u64))
        .chain([row(OLD_ROW_KEY.into(), ORIGINAL_SCORE)])
        .collect();
    SceneEntityRecord::new(id("anchor"))
        .with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("wire_rows", ComponentValue::List(rows)),
                    ("wire_note", ComponentValue::String(ORIGINAL_NOTE.into())),
                ]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
        )
        .with_component(
            ComponentTypeId::new("consumer.Unknown"),
            SceneComponentRecord::new(
                ComponentSchemaVersion(7),
                ComponentValue::String("preserve exactly".into()),
            ),
        )
}

fn selected_read(
    basis: &EditorDocumentQueryBasis,
    property: ComponentPropertyPath,
) -> EditorPropertyRead {
    EditorPropertyRead {
        target: EditorPropertyTarget {
            basis: basis.clone(),
            entity: id("anchor"),
            component: ComponentTypeId::new(COMPONENT),
            property,
        },
        limits: EditorPropertyReadLimits::new(128, 1, 32)
            .unwrap()
            .with_scan_limits(KEYED_SCAN_ENTRIES, 4 * 1024)
            .unwrap(),
    }
}

fn keyed_read(basis: &EditorDocumentQueryBasis, key: &str, member: &str) -> EditorPropertyRead {
    selected_read(
        basis,
        ComponentPropertyPath::new(ComponentFieldId::new("rows")).with_steps([
            ComponentPropertyStep::ListItem {
                by: ComponentFieldId::new("id"),
                key: ComponentPropertyKey::String(key.into()),
            },
            ComponentPropertyStep::Member(ComponentFieldId::new(member)),
        ]),
    )
}

fn note_read(basis: &EditorDocumentQueryBasis) -> EditorPropertyRead {
    selected_read(
        basis,
        ComponentPropertyPath::new(ComponentFieldId::new("note")),
    )
}

fn assert_selected_scalar(
    observed: &EditorPropertyObservation,
    request: &EditorPropertyRead,
    expected: ComponentValue,
) {
    assert_eq!(observed.target, request.target);
    assert!(observed.metadata_bytes <= request.limits.max_metadata_bytes());
    assert_eq!(observed.value_nodes, 1);
    assert_eq!(
        observed.value_logical_bytes,
        expected.cost().logical_bytes()
    );
    assert!(observed.scanned_entries <= request.limits.max_scan_entries());
    assert!(observed.scanned_logical_bytes <= request.limits.max_scan_logical_bytes());
    assert_eq!(observed.value, EditorPropertyValue::Present(expected));
}

fn assert_preview_leaf_page(
    prepared: &PreparedEditorDocumentEdit,
    request: &EditorPropertyRead,
    expected: ComponentValue,
) {
    let page = prepared
        .preview_property_page(root_page_request(request.target.clone(), request.limits))
        .unwrap();
    assert_eq!(page.target(), &request.target);
    assert!(page.path().is_empty());
    assert!(page.next.is_none());
    assert_page_limits(&page, request.limits);
    let expected = match expected {
        ComponentValue::String(text) => EditorPropertyPageData::Text {
            total_bytes: text.len(),
            start: 0,
            text,
        },
        value => EditorPropertyPageData::Scalar(value),
    };
    assert_eq!(page.value, EditorPropertyPageValue::Present(expected));
}

fn property_edit(request: &EditorPropertyRead, value: ComponentValue) -> AuthoringPropertyEdit {
    AuthoringPropertyEdit {
        entity: request.target.entity.clone(),
        component: request.target.component.clone(),
        component_version: ComponentSchemaVersion::ONE,
        property: request.target.property.clone(),
        value,
    }
}

fn missing_row(key: &str) -> EditorDocumentQueryRejection {
    EditorDocumentQueryRejection::Source(AuthoringReadError::Property(
        ComponentPropertyError::ListItemNotFound {
            step: 0,
            key: ComponentPropertyKey::String(key.into()),
        },
    ))
}

fn prepare_keyed_edit(
    context: &EditorPanelContext<'_, '_>,
    captured: &EditorDocumentQueryBasis,
) -> (PreparedEditorDocumentEdit, EditorPropertyPageRead) {
    let old_score = keyed_read(captured, OLD_ROW_KEY, "score");
    let new_score = keyed_read(captured, NEW_ROW_KEY, "score");
    let old_key = keyed_read(captured, OLD_ROW_KEY, "id");
    let new_key = keyed_read(captured, NEW_ROW_KEY, "id");
    let mut rows = old_key.target.clone();
    rows.property.steps.clear();
    let limits = EditorPropertyReadLimits::new(128, 2, 32)
        .unwrap()
        .with_scan_limits(32, 1024)
        .unwrap();
    let first = context
        .read_document_property_page(root_page_request(rows.clone(), limits))
        .unwrap();
    assert_page_limits(&first, limits);
    assert_eq!(first.target(), &rows);
    assert_eq!(
        first.value,
        EditorPropertyPageValue::Present(EditorPropertyPageData::List {
            total_items: KEYED_ROW_COUNT,
            start: 0,
            items: vec![EditorPropertyPageItem::Map { len: 2 }],
        })
    );
    let continuation = EditorPropertyPageRead {
        position: EditorPropertyPagePosition::Continue(first.next.unwrap()),
        limits,
    };

    let before = context.read_document_property(old_score.clone()).unwrap();
    assert_selected_scalar(&before, &old_score, ComponentValue::U64(ORIGINAL_SCORE));
    let edits = [
        property_edit(&old_score, ComponentValue::U64(INTERMEDIATE_ROW_SCORE)),
        property_edit(&old_key, ComponentValue::String(NEW_ROW_KEY.into())),
        property_edit(&new_score, ComponentValue::U64(UPDATED_SCORE)),
    ];
    let mut missing_final = edits.clone();
    missing_final[2].property = old_score.target.property.clone();
    let mut duplicate_final = edits.clone();
    duplicate_final[2] = property_edit(&new_key, ComponentValue::String(FIRST_ROW_KEY.into()));
    for (invalid_edits, error_kind) in [
        (missing_final, "list-item-not-found"),
        (duplicate_final, "ambiguous-list-item"),
    ] {
        let error = context
            .prepare_authoring_property_edit(captured, &invalid_edits)
            .unwrap_err();
        let EditorEditPreparationError::Document(rejected) = error else {
            panic!("the final write must fail with {error_kind}: {error:?}");
        };
        assert!(!rejected.applied);
        assert!(rejected.patch_report.as_ref().unwrap().inverse.is_none());
        assert!(rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-property-edit-rejected"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "operation-index"
                        && field.value() == DiagnosticValueRef::Unsigned(2)
                })
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "property-error-kind"
                        && field.value() == DiagnosticValueRef::Identifier(error_kind)
                })
        }));
        assert_eq!(
            context.document_query_basis(captured.document).unwrap(),
            *captured
        );
        assert_eq!(
            context.read_document_property(old_score.clone()),
            Ok(before.clone())
        );
        assert_keyed_rows(
            captured,
            |read| context.read_document_property(read),
            OLD_ROW_KEY,
            ORIGINAL_SCORE,
        );
    }

    let prepared = context
        .prepare_authoring_property_edit(captured, &edits)
        .expect("the leaf writes must fit without replaying the authored list");
    let replace = |edit: AuthoringPropertyEdit| ScenePatchOperation::ReplaceProperty {
        entity: edit.entity,
        component: edit.component,
        component_version: edit.component_version,
        property: edit.property,
        value: edit.value,
    };
    assert_eq!(prepared.patch().operations.as_slice(), &edits.map(replace));
    let inverse = [
        property_edit(&new_score, ComponentValue::U64(INTERMEDIATE_ROW_SCORE)),
        property_edit(&new_key, ComponentValue::String(OLD_ROW_KEY.into())),
        property_edit(&old_score, ComponentValue::U64(ORIGINAL_SCORE)),
    ];
    assert_eq!(
        prepared.inverse().operations.as_slice(),
        &inverse.map(replace)
    );
    assert_eq!(context.read_document_property(old_score), Ok(before));
    assert_keyed_rows(
        captured,
        |read| prepared.preview_property(read),
        NEW_ROW_KEY,
        UPDATED_SCORE,
    );
    assert_preview_leaf_page(&prepared, &new_score, ComponentValue::U64(UPDATED_SCORE));
    (prepared, continuation)
}

fn prepare_note_edit(
    context: &EditorPanelContext<'_, '_>,
    captured: &EditorDocumentQueryBasis,
) -> PreparedEditorDocumentEdit {
    assert_keyed_rows(
        captured,
        |read| context.read_document_property(read),
        OLD_ROW_KEY,
        ORIGINAL_SCORE,
    );
    let note = note_read(captured);
    let original = ComponentValue::String(ORIGINAL_NOTE.into());
    let updated = ComponentValue::String(UPDATED_NOTE.into());
    let edit = property_edit(&note, updated.clone());
    let prepared = context
        .prepare_authoring_property_edit(captured, &[edit])
        .expect("a scalar edit must fit beside the authored list");
    let set_note = |value| ScenePatchOperation::SetField {
        entity: note.target.entity.clone(),
        component: note.target.component.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: note.target.property.root.clone(),
        value,
    };
    assert_eq!(
        prepared.patch().operations.as_slice(),
        &[set_note(updated.clone())]
    );
    assert_eq!(
        prepared.inverse().operations.as_slice(),
        &[set_note(original.clone())]
    );
    assert_selected_scalar(
        &context.read_document_property(note.clone()).unwrap(),
        &note,
        original,
    );
    assert_selected_scalar(
        &prepared.preview_property(note.clone()).unwrap(),
        &note,
        updated.clone(),
    );
    assert_preview_leaf_page(&prepared, &note, updated);
    assert_keyed_rows(
        captured,
        |read| prepared.preview_property(read),
        OLD_ROW_KEY,
        ORIGINAL_SCORE,
    );
    prepared
}

fn assert_keyed_rows(
    captured: &EditorDocumentQueryBasis,
    read: impl Fn(EditorPropertyRead) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection>,
    last_key: &str,
    last_score: u64,
) {
    for (key, score) in [(FIRST_ROW_KEY, 0), (last_key, last_score)] {
        let request = keyed_read(captured, key, "score");
        assert_selected_scalar(
            &read(request.clone()).unwrap(),
            &request,
            ComponentValue::U64(score),
        );
    }
    let request = keyed_read(captured, last_key, "id");
    assert_selected_scalar(
        &read(request.clone()).unwrap(),
        &request,
        ComponentValue::String(last_key.into()),
    );
    let absent_key = if last_key == OLD_ROW_KEY {
        NEW_ROW_KEY
    } else {
        OLD_ROW_KEY
    };
    assert_eq!(
        read(keyed_read(captured, absent_key, "score")),
        Err(missing_row(absent_key))
    );
}

fn assert_keyed_state(app: &EditorApplication, document: EditorDocumentId, key: &str, score: u64) {
    let captured = basis(app, document);
    assert_keyed_rows(
        &captured,
        |read| app.read_document_property(read),
        key,
        score,
    );
    let request = note_read(&captured);
    assert_selected_scalar(
        &app.read_document_property(request.clone()).unwrap(),
        &request,
        ComponentValue::String(UPDATED_NOTE.into()),
    );
}

#[test]
fn a_registered_panel_round_trips_large_keyed_list_edits_on_a_nonactive_scene() {
    round_trip_keyed_edit(EditorDocumentKind::Scene, "scenes/keyed-rows.scene.json");
}

#[test]
fn a_registered_panel_round_trips_large_keyed_list_edits_on_a_nonactive_prefab() {
    round_trip_keyed_edit(EditorDocumentKind::Prefab, "prefabs/keyed-rows.prefab.json");
}

fn review_and_commit_keyed_edit(
    app: &mut EditorApplication,
    host: &mut EditorPanelHost<PropertyPagesPanel>,
    instance: EditorPanelInstanceId,
    document: EditorDocumentId,
    mut prepare: impl FnMut(
        &EditorPanelContext<'_, '_>,
        &EditorDocumentQueryBasis,
    ) -> PreparedEditorDocumentEdit,
) {
    let initial = frame(app);
    let active = initial.workspace().active_document.unwrap();
    let initial_tab = initial.workspace().document(document).unwrap();
    let mut token = None;
    let review = host
        .prepare(app, |panel, context| {
            assert!(context.model().document().is_none());
            assert_eq!(
                context.model().frame().workspace().active_document,
                Some(active)
            );
            let captured = context.document_query_basis(document).unwrap();
            let prepared = prepare(context, &captured);
            assert_eq!(prepared.document(), document);
            assert_eq!(prepared.document_kind(), initial_tab.kind);
            assert_eq!(prepared.base_revision(), captured.revision);
            assert!(panel.retained.replace(prepared).is_none());
            assert!(token.replace(context.instance_token()).is_none());
            panel.completion = None;
        })
        .unwrap();
    assert_eq!(review.submissions().len(), 0);
    host.submit_and_drive(app, review, Duration::ZERO);
    let token = token.unwrap();
    assert_eq!(token.instance(), instance);
    assert_eq!(
        frame(app).workspace().documents,
        initial.workspace().documents
    );
    assert_navigation(app, &initial, document);

    let commit = host
        .prepare(app, |panel, context| {
            assert!(context.matches_instance(&token));
            assert!(panel.completion.is_none());
            context
                .commit_authoring_edit(panel.retained.take().unwrap())
                .unwrap();
        })
        .unwrap();
    assert_eq!(commit.submissions().len(), 1);
    let (origin, submitted) = commit.submissions().next().unwrap();
    assert_eq!(origin, instance);
    assert_eq!(
        frame(app).workspace().documents,
        initial.workspace().documents
    );
    host.submit_and_drive(app, commit, Duration::ZERO);
    let edited = frame(app);
    let tab = edited.workspace().document(document).unwrap();
    host.inspect(app, &token, |panel, _| {
        let report = panel.completed(submitted);
        assert!(report.applied, "{:?}", report.diagnostics);
        assert_eq!(report.document, Some(document));
        assert_eq!(report.active_document, Some(active));
        assert_eq!(report.revision, Some(tab.revision));
    })
    .unwrap();
    assert_ne!(tab.revision, initial_tab.revision);
    assert_eq!(tab.revision.source_id(), initial_tab.revision.source_id());
    assert!(tab.dirty);
    assert_eq!(tab.saved_revision, initial_tab.saved_revision);
    assert_eq!(
        (tab.history.undo_depth, tab.history.redo_depth),
        (initial_tab.history.undo_depth + 1, 0)
    );
    assert_navigation(app, &initial, document);
}

fn round_trip_keyed_edit(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let (mut app, document, original, startup) = open_nonactive_source(
        &project,
        kind,
        source,
        keyed_rows_schema(),
        [keyed_rows_entity()],
    );
    let source_path = project.root.join(source);
    let initial = frame(&app);
    let active = initial.workspace().active_document.unwrap();
    let (mut host, instance) = register_panel(&mut app);
    let assert_unsaved = || {
        assert!(
            fs::read(&source_path).unwrap().as_slice() == original.as_bytes(),
            "the original source bytes must persist until Save"
        );
    };
    review_and_commit_keyed_edit(&mut app, &mut host, instance, document, prepare_note_edit);
    assert_navigation(&app, &initial, document);
    assert_unsaved();
    assert_keyed_state(&app, document, OLD_ROW_KEY, ORIGINAL_SCORE);

    let mut continuation = None;
    review_and_commit_keyed_edit(
        &mut app,
        &mut host,
        instance,
        document,
        |context, captured| {
            let (prepared, cursor) = prepare_keyed_edit(context, captured);
            assert!(continuation.replace(cursor).is_none());
            assert_unsaved();
            prepared
        },
    );
    assert_eq!(
        app.read_document_property_page(continuation.unwrap()),
        Err(EditorDocumentQueryRejection::StaleRevision)
    );
    assert_keyed_state(&app, document, NEW_ROW_KEY, UPDATED_SCORE);
    assert_navigation(&app, &initial, document);
    assert_unsaved();

    let mut expected: serde_json::Value = serde_json::from_str(&original).unwrap();
    let component = &mut expected["payload"]["entities"][0]["components"][COMPONENT]["value"];
    let values = &mut component["value"];
    let last = &values["wire_rows"]["value"][KEYED_ROW_COUNT - 1]["value"];
    assert_eq!(last["wire_key"]["value"], OLD_ROW_KEY);
    assert_eq!(last["wire_score"]["value"], ORIGINAL_SCORE);
    assert_eq!(values["wire_note"]["value"], ORIGINAL_NOTE);
    values["wire_note"]["value"] = UPDATED_NOTE.into();
    let set_expected_row = |expected: &mut serde_json::Value, key: &str, score: u64| {
        let component = &mut expected["payload"]["entities"][0]["components"][COMPONENT]["value"];
        let rows = component["value"]["wire_rows"]["value"]
            .as_array_mut()
            .unwrap();
        assert_eq!(rows.len(), KEYED_ROW_COUNT);
        let last = &mut rows.last_mut().unwrap()["value"];
        last["wire_key"]["value"] = key.into();
        last["wire_score"]["value"] = score.into();
    };
    set_expected_row(&mut expected, NEW_ROW_KEY, UPDATED_SCORE);

    let mut saved = save_exact_source(&mut app, document, &source_path, &expected);
    assert_navigation(&app, &initial, document);
    for (redo, key, score) in [
        (false, OLD_ROW_KEY, ORIGINAL_SCORE),
        (true, NEW_ROW_KEY, UPDATED_SCORE),
    ] {
        let (checkpoint, _, disk_bytes) = &saved;
        let base_revision = checkpoint.revision;
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
        assert!(
            matches!(
                result.actions(),
                [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(
                    report,
                ))] if report.applied && report.document == Some(document)
                    && report.active_document == Some(active)
            ),
            "{result:?}"
        );
        assert_keyed_state(&app, document, key, score);
        let current = frame(&app);
        let tab = current.workspace().document(document).unwrap();
        assert!(tab.dirty);
        assert_eq!(
            (tab.history.undo_depth, tab.history.redo_depth),
            (1 + usize::from(redo), usize::from(!redo))
        );
        assert_navigation(&app, &initial, document);
        assert!(
            &fs::read(&source_path).unwrap() == disk_bytes,
            "history replay must leave the prior checkpoint on disk until Save"
        );
        set_expected_row(&mut expected, key, score);
        saved = save_exact_source(&mut app, document, &source_path, &expected);
        assert_navigation(&app, &initial, document);
    }

    let (saving, digest, saved_bytes) = saved;
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
        app.read_document_property(keyed_read(&saving, NEW_ROW_KEY, "score")),
        Err(EditorDocumentQueryRejection::StaleIncarnation)
    );
    assert_keyed_state(&app, document, NEW_ROW_KEY, UPDATED_SCORE);
    assert_navigation(&app, &initial, document);
    assert!(
        fs::read(source_path).unwrap() == saved_bytes,
        "Reopen must retain the final saved source bytes"
    );
    assert_eq!(
        fs::read_to_string(project.root.join(STARTUP)).unwrap(),
        startup
    );
    assert!(
        app.close(),
        "the keyed-list project did not physically retire"
    );
}

fn save_exact_source(
    app: &mut EditorApplication,
    document: EditorDocumentId,
    path: &std::path::Path,
    expected: &serde_json::Value,
) -> (EditorDocumentQueryBasis, EditorDocumentDigest, Vec<u8>) {
    let saving = basis(app, document);
    let saved = persist(
        app,
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
        panic!("the keyed-list source must save: {saved:?}");
    };
    assert_eq!(saved_document, document);
    assert_eq!(revision, saving.revision);
    let current = frame(app);
    let tab = current.workspace().document(document).unwrap();
    assert_eq!(tab.saved_revision, saving.revision);
    assert!(!tab.dirty);
    let bytes = fs::read(path).unwrap();
    assert!(
        &serde_json::from_slice::<serde_json::Value>(&bytes).unwrap() == expected,
        "the complete normalized source must match after saving revision {revision:?}"
    );
    (saving, digest, bytes)
}
