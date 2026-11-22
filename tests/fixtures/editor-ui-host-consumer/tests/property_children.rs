//! An independent panel discovers collection selectors before preparing a non-active source edit.
#![forbid(unsafe_code)]
#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]

#[path = "property_children/fixture.rs"]
mod fixture;

use std::{fs, rc::Rc, time::Duration};

use nara::{
    project_host::EditorEditPreparationError,
    reflect::{
        ComponentFieldId, ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath,
        ComponentPropertyStep, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind,
    },
    scene::{
        AuthoringPropertyEdit, AuthoringReadError, AuthoringRecordReadiness, PrefabDocument,
        PrefabDocumentCandidate, SceneDocumentCandidate,
    },
    tooling::{
        EditorAction, EditorActionOutcome, EditorAuthoringPage, EditorAuthoringProjection,
        EditorAuthoringPropertyStatus, EditorAuthoringQuery, EditorAuthoringQueryLimits,
        EditorAuthoringQueryPosition, EditorAuthoringQueryTarget, EditorAuthoringRow,
        EditorDocumentId, EditorDocumentKind, EditorDocumentMutation, EditorDocumentOpenResult,
        EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorEntityQuery,
        EditorEntityQueryLimits, EditorEntityQueryPosition, EditorFrameModel,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorPersistenceView, EditorProjectBrowserAction,
        EditorProjectBrowserActionOutcome, EditorPropertyRead, EditorPropertyReadLimits,
        EditorPropertyTarget, EditorPropertyValue, EditorReopenTarget,
    },
};
use nara_editor::{
    EditorApplication, EditorApplicationActionDispatch, EditorPanelHost, PanelActionDispatch,
    panels::EditorPanelContext,
};
use nara_editor_ui_host_consumer::{
    DraftPanel, Evidence,
    project::{
        Application, Project, SOURCE, current, dispatch, entity, persistence, register, wait,
    },
};
use nara_text::bundled_font_face;
use nara_ui_kernel::{UiCompiler, UiEnvironment, UiSemanticRole, UiSurfaceId, UiTheme};

const SCAN_ENTRIES: usize = 128;
const METADATA_BYTES: usize = 2048;
const SCAN_BYTES: usize = 4096;
const LARGE_SCAN_BYTES: usize = 256 * 1024;
const PREFAB_SOURCE: &str = "prefabs/property-children.prefab.json";

fn limits(metadata: usize) -> EditorAuthoringQueryLimits {
    EditorAuthoringQueryLimits::new(SCAN_ENTRIES, 1, metadata)
        .unwrap()
        .with_scan_logical_bytes(SCAN_BYTES)
        .unwrap()
}

fn discover(
    context: &EditorPanelContext<'_, '_>,
    target: EditorAuthoringQueryTarget,
) -> Vec<EditorAuthoringPage> {
    let mut position = EditorAuthoringQueryPosition::Start(target.clone());
    let mut pages = Vec::new();
    for _ in 0..16 {
        let page = context
            .query_document_authoring(EditorAuthoringQuery {
                position,
                limits: limits(METADATA_BYTES),
            })
            .unwrap();
        assert_eq!(page.target.as_ref(), &target);
        assert!(page.rows.len() <= 1);
        assert!(page.scanned_entries <= SCAN_ENTRIES);
        assert!(page.metadata_bytes <= METADATA_BYTES);
        let next = page.next.clone();
        pages.push(page);
        let Some(next) = next else {
            return pages;
        };
        position = EditorAuthoringQueryPosition::Continue(next);
    }
    panic!("the small fixture's property discovery did not terminate");
}

fn rows(pages: &[EditorAuthoringPage]) -> impl Iterator<Item = &EditorAuthoringRow> {
    pages.iter().flat_map(|page| &page.rows)
}

fn select_property(
    pages: &[EditorAuthoringPage],
    kind: ComponentValueKind,
    select: impl Fn(&ComponentPropertyStep) -> bool,
) -> ComponentPropertyPath {
    let mut found = rows(pages).filter_map(|row| match row {
        EditorAuthoringRow::Property {
            property,
            kind: Some(actual),
            edit_capability: true,
            status: EditorAuthoringPropertyStatus::Available,
        } if *actual == kind && select(property.child_step()) => Some(property),
        _ => None,
    });
    let property = found.next().expect("an editable child must be discovered");
    assert!(found.next().is_none(), "the child selection must be unique");
    property.to_path()
}

fn assert_child(parent: &ComponentPropertyPath, child: &ComponentPropertyPath) {
    assert_eq!(child.root, parent.root);
    assert_eq!(child.steps.len(), parent.steps.len() + 1);
    assert!(child.steps.starts_with(&parent.steps));
}

fn selected_read(target: EditorPropertyTarget) -> EditorPropertyRead {
    let metadata = target.entity.as_str().len()
        + target.component.as_str().len()
        + target.property.logical_bytes().unwrap();
    let scan_bytes = if metadata > METADATA_BYTES {
        LARGE_SCAN_BYTES
    } else {
        SCAN_BYTES
    };
    EditorPropertyRead {
        target,
        limits: EditorPropertyReadLimits::new(metadata, 1, 8)
            .unwrap()
            .with_scan_limits(SCAN_ENTRIES, scan_bytes)
            .unwrap(),
    }
}

fn discover_large_map(
    basis: &EditorDocumentQueryBasis,
    query: impl Fn(EditorAuthoringQuery) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection>,
) -> (EditorPropertyRead, EditorAuthoringQuery) {
    let [first_key, second_key] = fixture::large_keys();
    assert_eq!(first_key.len(), fixture::LARGE_KEY_BYTES);
    assert_eq!(second_key.len(), fixture::LARGE_KEY_BYTES);
    let limits = |rows, metadata| {
        EditorAuthoringQueryLimits::new(256, rows, metadata)
            .unwrap()
            .with_scan_logical_bytes(LARGE_SCAN_BYTES)
            .unwrap()
    };
    let wide = limits(256, 64 * 1024);
    let entity = entity("anchor");
    let component = ComponentTypeId::new(fixture::COMPONENT);
    let start = |property: ComponentPropertyPath| EditorAuthoringQueryTarget {
        basis: basis.clone(),
        entity: entity.clone(),
        projection: EditorAuthoringProjection::Properties {
            component: component.clone(),
            property: property.into(),
        },
    };
    let parent = query(EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(start(
            ComponentPropertyPath::new(ComponentFieldId::new("settings")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("scores")),
            ]),
        )),
        limits: wide,
    })
    .unwrap();
    assert_eq!(parent.rows.len(), 1);
    assert!(parent.next.is_none());
    let map = select_property(
        &[parent],
        ComponentValueKind::Map,
        |step| matches!(step, ComponentPropertyStep::MapEntry(key) if key.len() == fixture::PARENT_KEY_BYTES),
    );
    let target_bytes =
        entity.as_str().len() + component.as_str().len() + map.logical_bytes().unwrap();
    let target = start(map.clone());
    let assert_page = |page: &EditorAuthoringPage,
                       limits: EditorAuthoringQueryLimits,
                       keys: &[&str]| {
        assert_eq!(page.target.as_ref(), &target);
        assert!(page.scanned_entries <= limits.max_scan_entries());
        assert!(page.scanned_logical_bytes <= limits.max_scan_logical_bytes());
        assert!(page.metadata_bytes <= limits.max_metadata_bytes());
        assert_eq!(
            page.metadata_bytes,
            target_bytes + keys.iter().map(|key| key.len()).sum::<usize>()
        );
        assert_eq!(page.rows.len(), keys.len());
        for (row, expected) in page.rows.iter().zip(keys) {
            let EditorAuthoringRow::Property {
                property,
                kind: Some(ComponentValueKind::U64),
                edit_capability: true,
                status: EditorAuthoringPropertyStatus::Available,
            } = row
            else {
                panic!("expected an editable Map scalar");
            };
            assert!(
                matches!(property.child_step(), ComponentPropertyStep::MapEntry(key) if key == *expected),
                "Map pagination must preserve key order and bytes"
            );
        }
    };
    let first = query(EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(target.clone()),
        limits: wide,
    })
    .unwrap();
    assert_page(&first, wide, &[&first_key]);
    let resume = |page: &EditorAuthoringPage, limits| EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Continue(page.next.clone().unwrap()),
        limits,
    };
    let continuation = resume(&first, limits(2, target_bytes + second_key.len() + 1));
    let rest = query(continuation.clone()).unwrap();
    assert_page(&rest, continuation.limits, &[&second_key, "z"]);
    assert!(rest.next.is_none());
    let single = target_bytes + second_key.len();
    assert_eq!(
        query(resume(&first, limits(256, single - 1))),
        Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
    );
    let second = query(resume(&first, limits(256, single))).unwrap();
    assert_page(&second, limits(256, single), &[&second_key]);
    let tail = query(resume(&second, limits(256, target_bytes + 1))).unwrap();
    assert_page(&tail, limits(256, target_bytes + 1), &["z"]);
    assert!(tail.next.is_none());

    let property = select_property(&[first], ComponentValueKind::U64, |_| true);
    assert_child(&map, &property);
    let read = selected_read(EditorPropertyTarget {
        basis: basis.clone(),
        entity,
        component,
        property,
    });
    (read, continuation)
}

fn discover_leaf(
    context: &EditorPanelContext<'_, '_>,
    document: EditorDocumentId,
    large_map: bool,
) -> (
    EditorPropertyRead,
    ComponentSchemaVersion,
    EditorAuthoringQuery,
) {
    assert!(context.model().document().is_none());
    let basis = context.document_query_basis(document).unwrap();
    assert!(basis.schema.is_some());
    let mut position = EditorEntityQueryPosition::Start(basis.clone());
    let mut entities = Vec::new();
    let mut entity_pages = 0;
    loop {
        entity_pages += 1;
        assert!(
            entity_pages <= 3,
            "entity discovery must make bounded progress"
        );
        let page = context
            .query_document_entities(EditorEntityQuery {
                position,
                limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
            })
            .unwrap();
        assert_eq!(page.basis, basis);
        assert!(page.entities.len() <= 1);
        assert!(page.scanned_records <= 1);
        entities.extend(page.entities);
        let Some(next) = page.next else {
            break;
        };
        assert!(entities.len() <= 3);
        position = EditorEntityQueryPosition::Continue(next);
    }
    assert_eq!(
        entities,
        [entity("anchor"), entity("tail"), entity("untouched")]
    );
    let entity = entities.remove(0);
    let start = |projection| EditorAuthoringQueryTarget {
        basis: basis.clone(),
        entity: entity.clone(),
        projection,
    };
    let components = discover(context, start(EditorAuthoringProjection::Components));
    assert_eq!(components.len(), 2);
    let component_rows = rows(&components).collect::<Vec<_>>();
    let [
        EditorAuthoringRow::Component {
            component,
            version,
            readiness: AuthoringRecordReadiness::Available,
        },
        EditorAuthoringRow::Component {
            component: unknown,
            version: unknown_version,
            readiness: AuthoringRecordReadiness::UnknownSchema,
        },
    ] = component_rows.as_slice()
    else {
        panic!("actual component inventory must preserve the unknown record");
    };
    let (component, version, unknown) = (component.clone(), *version, unknown.clone());
    assert_eq!(component.as_str(), fixture::COMPONENT);
    assert_eq!(unknown.as_str(), fixture::UNKNOWN);
    assert_eq!(*unknown_version, ComponentSchemaVersion(7));
    if large_map {
        let (read, continuation) =
            discover_large_map(&basis, |query| context.query_document_authoring(query));
        return (read, version, continuation);
    }
    let roots = discover(
        context,
        start(EditorAuthoringProjection::Roots {
            component: component.clone(),
        }),
    );
    let [
        EditorAuthoringRow::Root {
            field,
            kind: ComponentValueKind::Map,
            required: true,
            edit_capability: true,
        },
    ] = roots[0].rows.as_slice()
    else {
        panic!("the admitted component must expose its editable Record root");
    };
    assert_eq!(roots.len(), 1);
    assert_eq!(field.as_str(), "settings");
    let root = ComponentPropertyPath::new(field.clone());
    let children = |property: &ComponentPropertyPath| {
        discover(
            context,
            start(EditorAuthoringProjection::Properties {
                component: component.clone(),
                property: property.clone().into(),
            }),
        )
    };
    let root_children = children(&root);
    assert_eq!(root_children.len(), 4);
    assert!(rows(&root_children).any(|row| matches!(
        row,
        EditorAuthoringRow::Property {
            property,
            kind: None,
            edit_capability: false,
            status: EditorAuthoringPropertyStatus::Retired,
        } if matches!(property.child_step(), ComponentPropertyStep::Member(id) if id.as_str() == "retired")
    )));
    let sections = select_property(
        &root_children,
        ComponentValueKind::Map,
        |step| matches!(step, ComponentPropertyStep::Member(id) if id.as_str() == "sections"),
    );
    assert_child(&root, &sections);
    assert!(matches!(
        sections.steps.last(),
        Some(ComponentPropertyStep::Member(id)) if id.as_str() == "sections"
    ));
    let section_children = children(&sections);
    let groups = select_property(&section_children, ComponentValueKind::Map, |_| true);
    assert_child(&sections, &groups);
    assert!(matches!(
        groups.steps.last(),
        Some(ComponentPropertyStep::Member(id)) if id.as_str() == "groups"
    ));
    let map_children = children(&groups);
    assert_eq!(map_children.len(), 2);
    let collection = select_property(
        &map_children,
        ComponentValueKind::List,
        |step| matches!(step, ComponentPropertyStep::MapEntry(key) if key == fixture::MAP_KEY),
    );
    assert_child(&groups, &collection);
    let list_children = children(&collection);
    assert_eq!(list_children.len(), 2);
    let item = select_property(&list_children, ComponentValueKind::Map, |step| {
        matches!(step, ComponentPropertyStep::ListItem {
            by, key: ComponentPropertyKey::String(key),
        } if by.as_str() == "entry_id" && key == fixture::ITEM_KEY)
    });
    assert_child(&collection, &item);
    let item_children = children(&item);
    assert_eq!(rows(&item_children).count(), 3);
    let leaf = select_property(&item_children, ComponentValueKind::U64, |_| true);
    assert_child(&item, &leaf);
    assert!(matches!(
        leaf.steps.last(),
        Some(ComponentPropertyStep::Member(id)) if id.as_str() == "score"
    ));

    let first = &map_children[0];
    let request = |metadata| EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Start(first.target.as_ref().clone()),
        limits: limits(metadata),
    };
    assert_eq!(
        context
            .query_document_authoring(request(first.metadata_bytes))
            .unwrap(),
        *first
    );
    assert_eq!(
        context.query_document_authoring(request(first.metadata_bytes - 1)),
        Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
    );
    let mut too_little_scan = request(METADATA_BYTES);
    too_little_scan.limits = limits(METADATA_BYTES).with_scan_logical_bytes(1).unwrap();
    assert_eq!(
        context.query_document_authoring(too_little_scan),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::Property(ComponentPropertyError::ReadBudgetExceeded),
        ))
    );
    assert_eq!(
        context.query_document_authoring(EditorAuthoringQuery {
            position: EditorAuthoringQueryPosition::Start(start(
                EditorAuthoringProjection::Properties {
                    component: unknown,
                    property: root.clone().into(),
                },
            )),
            limits: limits(METADATA_BYTES),
        }),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::UnavailableComponent {
                readiness: AuthoringRecordReadiness::UnknownSchema,
            },
        ))
    );
    let read = selected_read(EditorPropertyTarget {
        basis,
        entity,
        component,
        property: leaf,
    });
    let mut whole_root = read.clone();
    whole_root.target.property = root;
    assert_eq!(
        context.read_document_property(whole_root),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    let mut too_few_bytes = read.clone();
    too_few_bytes.limits =
        EditorPropertyReadLimits::new(read.limits.max_metadata_bytes(), 1, 7).unwrap();
    assert_eq!(
        context.read_document_property(too_few_bytes),
        Err(EditorDocumentQueryRejection::ValueBudgetExceeded)
    );
    let mut too_little_metadata = read.clone();
    too_little_metadata.limits =
        EditorPropertyReadLimits::new(read.limits.max_metadata_bytes() - 1, 1, 8).unwrap();
    assert_eq!(
        context.read_document_property(too_little_metadata),
        Err(EditorDocumentQueryRejection::MetadataBudgetExceeded)
    );
    let mut too_few_entries = read.clone();
    too_few_entries.limits = read.limits.with_scan_limits(1, SCAN_BYTES).unwrap();
    assert_eq!(
        context.read_document_property(too_few_entries),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::Property(ComponentPropertyError::ReadBudgetExceeded),
        ))
    );
    let continuation = EditorAuthoringQuery {
        position: EditorAuthoringQueryPosition::Continue(first.next.clone().unwrap()),
        limits: limits(METADATA_BYTES),
    };
    (read, version, continuation)
}

fn panel_action(
    host: &mut EditorPanelHost<DraftPanel>,
    application: &mut EditorApplication,
    evidence: &Evidence,
    action: EditorAction,
) -> PanelActionDispatch {
    let count = evidence.finished.borrow().len();
    let mut action = Some(action);
    let mut id = None;
    let batch = host
        .prepare(application, |_, context| {
            id = Some(context.submit(action.take().unwrap()).unwrap());
        })
        .unwrap();
    let id = id.unwrap();
    assert_eq!(
        batch.submissions().map(|(_, id)| id).collect::<Vec<_>>(),
        [id]
    );
    assert_eq!(evidence.finished.borrow().len(), count);
    host.submit_and_drive(application, batch, Duration::ZERO);
    let finished = evidence.finished.borrow();
    assert_eq!(finished.len(), count + 1);
    assert_eq!((finished[count].0, finished[count].1), (1, id));
    assert_eq!(
        finished
            .iter()
            .filter(|(_, actual, _)| *actual == id)
            .count(),
        1
    );
    finished[count].2.clone()
}

fn open_nonactive_source(application: &mut EditorApplication, source: &str) -> EditorDocumentId {
    let active = current(application).workspace().active_document.unwrap();
    let opening = dispatch(
        application,
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
        panic!("Browser did not accept the source: {opening:?}");
    };
    wait(application, |frame| {
        frame.document_open().result().is_some()
    });
    let Some(EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    }) = current(application).document_open().result()
    else {
        panic!("Browser did not open {source}");
    };
    assert_eq!(*request, completed);
    dispatch(
        application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
            request: completed,
        }),
    );
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    );
    assert_eq!(
        current(application).workspace().active_document,
        Some(active)
    );
    document
}

fn applied(result: PanelActionDispatch, document: EditorDocumentId) {
    let PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) = result else {
        panic!("the exact panel result must be a workspace report");
    };
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(report.document, Some(document));
}

fn persist(
    host: &mut EditorPanelHost<DraftPanel>,
    application: &mut EditorApplication,
    evidence: &Evidence,
    action: EditorPersistenceAction,
) -> EditorPersistenceResult {
    assert_eq!(
        current(application).persistence(),
        EditorPersistenceView::Idle
    );
    let result = panel_action(
        host,
        application,
        evidence,
        EditorAction::Persistence(action),
    );
    let attempt = current(application).persistence().attempt().unwrap();
    assert_eq!(
        result,
        PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { attempt },
        ))
    );
    let result = persistence(application);
    assert_eq!(
        current(application).persistence(),
        EditorPersistenceView::Terminal { attempt, result }
    );
    assert_eq!(
        panel_action(
            host,
            application,
            evidence,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
        ),
        PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Acknowledged { attempt },
        ))
    );
    assert_eq!(
        current(application).persistence(),
        EditorPersistenceView::Idle
    );
    result
}

fn assert_leaf(
    host: &mut EditorPanelHost<DraftPanel>,
    application: &EditorApplication,
    target: &EditorPropertyTarget,
    expected: u64,
) {
    let frame = current(application);
    drop(
        host.prepare(application, |_, context| {
            let mut target = target.clone();
            target.basis = context.document_query_basis(target.basis.document).unwrap();
            let request = selected_read(target);
            let observation = context.read_document_property(request.clone()).unwrap();
            assert_eq!(observation.target, request.target);
            assert_eq!(
                observation.value,
                EditorPropertyValue::Present(ComponentValue::U64(expected))
            );
            assert_eq!(
                (observation.value_nodes, observation.value_logical_bytes),
                (1, 8)
            );
            assert_eq!(
                observation.metadata_bytes,
                request.limits.max_metadata_bytes()
            );
            assert!(observation.scanned_entries <= SCAN_ENTRIES);
            assert!(observation.scanned_logical_bytes <= request.limits.max_scan_logical_bytes());
        })
        .unwrap(),
    );
    assert_eq!(current(application).workspace(), frame.workspace());
}

fn assert_history(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    dirty: bool,
    undo: usize,
    redo: usize,
) {
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.dirty, dirty);
    assert_eq!(tab.history.undo_depth, undo);
    assert_eq!(tab.history.redo_depth, redo);
}

#[test]
fn a_public_panel_discovers_nested_collection_children_and_restores_its_saved_source() {
    round_trip(EditorDocumentKind::Scene, false);
}

#[test]
fn a_public_panel_pages_large_map_keys_in_a_nonactive_scene() {
    round_trip(EditorDocumentKind::Scene, true);
}

#[test]
fn a_public_panel_pages_large_map_keys_in_a_nonactive_prefab() {
    round_trip(EditorDocumentKind::Prefab, true);
}

fn round_trip(kind: EditorDocumentKind, large_map: bool) {
    let project = Project::new();
    let source = |score| {
        if large_map {
            fixture::source(fixture::ORIGINAL, score)
        } else {
            fixture::source(score, fixture::ORIGINAL)
        }
    };
    fs::write(
        project.root().join("schema/authoring.bundle.json"),
        fixture::schema().to_json_string().unwrap(),
    )
    .unwrap();
    let original = source(fixture::ORIGINAL);
    let (relative_path, original) = match kind {
        EditorDocumentKind::Scene => (SOURCE, original.to_json_string()),
        EditorDocumentKind::Prefab => (
            PREFAB_SOURCE,
            PrefabDocument::new(original.placements, original.entities).to_json_string(),
        ),
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    let source_path = project.root().join(relative_path);
    let original_bytes = original.unwrap().into_bytes();
    fs::write(&source_path, &original_bytes).unwrap();
    let startup_path = project.root().join("scenes/startup.scene.json");
    let startup_bytes = fs::read(&startup_path).unwrap();
    let mut application = Application::open(&project);
    let document = open_nonactive_source(&mut application, relative_path);
    let active = current(&application).workspace().active_document.unwrap();
    assert_ne!(active, document);
    let selection = dispatch(
        &mut application,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(entity("observer")),
        }),
    );
    assert!(matches!(selection.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
    ] if report.applied));
    let initial = current(&application);
    let initial_tab = initial.workspace().document(document).unwrap();
    assert_eq!(initial_tab.kind, kind);
    assert!(initial_tab.source_binding.is_some());
    assert_history(&initial, document, false, 0, 0);
    let assert_navigation = |application: &EditorApplication| {
        let frame = current(application);
        assert_eq!(frame.workspace().active_document, Some(active));
        assert_eq!(
            frame.workspace().active_scene,
            initial.workspace().active_scene
        );
        assert_eq!(
            frame.scene_preview_document(),
            initial.scene_preview_document()
        );
        assert_eq!(
            frame.workspace().document(active),
            initial.workspace().document(active)
        );
        assert_eq!(
            frame.workspace().documents.len(),
            initial.workspace().documents.len()
        );
        assert_eq!(
            frame.workspace().document(document).unwrap().source_binding,
            initial_tab.source_binding
        );
        assert_eq!(fs::read(&startup_path).unwrap(), startup_bytes);
    };
    let mut host = EditorPanelHost::new();
    let evidence = Rc::new(Evidence::default());
    let instance = register(
        &mut host,
        &mut application,
        &evidence,
        "consumer.property-children",
    );
    let mut discovered = None;
    let mut prepared = None;
    let mut stale = None;
    let mut token = None;
    let mut snapshot = None;
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    drop(
        host.prepare(&application, |panel, context| {
            let (read, version, continuation) = discover_leaf(context, document, large_map);
            let observation = context.read_document_property(read.clone()).unwrap();
            let EditorPropertyValue::Present(ComponentValue::U64(original)) = observation.value
            else {
                panic!("the discovered scalar must be present in the source");
            };
            assert_eq!(observation.target, read.target);
            assert_eq!(original, fixture::ORIGINAL);
            assert_eq!(
                (observation.value_nodes, observation.value_logical_bytes),
                (1, 8)
            );
            assert_eq!(observation.metadata_bytes, read.limits.max_metadata_bytes());
            assert!(observation.scanned_entries <= SCAN_ENTRIES);
            assert!(observation.scanned_logical_bytes <= read.limits.max_scan_logical_bytes());
            let edited = original.checked_add(2).unwrap();
            let edit = AuthoringPropertyEdit {
                entity: observation.target.entity.clone(),
                component: observation.target.component.clone(),
                component_version: version,
                property: observation.target.property.clone(),
                value: ComponentValue::U64(edited),
            };
            let basis = &read.target.basis;
            let cancelled = context
                .prepare_authoring_property_edit(basis, std::slice::from_ref(&edit))
                .unwrap();
            assert_eq!(cancelled.document(), document);
            assert_eq!(cancelled.document_kind(), kind);
            assert_eq!(cancelled.base_revision(), basis.revision);
            assert_eq!(cancelled.patch().operations.len(), 1);
            assert_eq!(cancelled.inverse().operations.len(), 1);
            drop(cancelled);
            let candidate = context
                .prepare_authoring_property_edit(basis, std::slice::from_ref(&edit))
                .unwrap();
            let sibling = context
                .prepare_authoring_property_edit(basis, std::slice::from_ref(&edit))
                .unwrap();
            assert_eq!(candidate.patch(), sibling.patch());
            assert_eq!(candidate.inverse(), sibling.inverse());
            if large_map {
                let preview = candidate.preview();
                let (candidate_read, candidate_next) =
                    discover_large_map(preview.basis(), |query| preview.query_authoring(query));
                assert_eq!(candidate_read, read);
                assert_eq!(
                    preview.read_property(candidate_read).unwrap().value,
                    EditorPropertyValue::Present(ComponentValue::U64(edited))
                );
                assert_eq!(
                    preview.query_authoring(continuation.clone()),
                    Err(EditorDocumentQueryRejection::WrongPreparedEdit)
                );
                assert_eq!(
                    context.query_document_authoring(candidate_next.clone()),
                    Err(EditorDocumentQueryRejection::WrongPreparedEdit)
                );
                assert_eq!(
                    sibling.preview().query_authoring(candidate_next),
                    Err(EditorDocumentQueryRejection::WrongPreparedEdit)
                );
            }
            prepared = Some(candidate);
            stale = Some(sibling);
            panel.replace_for_cpu(&edited.to_string()).unwrap();
            token = Some(context.instance_token());
            snapshot = Some(panel.compose(1).unwrap());
            discovered = Some((read, continuation, edit, edited));
        })
        .unwrap(),
    );
    let (old_read, continuation, edit, edited) = discovered.unwrap();
    let draft = edited.to_string();
    let original_basis = old_read.target.basis.clone();
    assert_leaf(&mut host, &application, &old_read.target, fixture::ORIGINAL);
    for revision in [2, 3] {
        drop(
            host.prepare(&application, |panel, context| {
                assert_eq!(
                    context.document_query_basis(document).unwrap(),
                    original_basis
                );
                assert_eq!(panel.serial(), 1);
                assert_eq!(panel.draft(), draft);
                assert_eq!(
                    prepared.as_ref().unwrap().base_revision(),
                    original_basis.revision
                );
                assert_eq!(
                    stale.as_ref().unwrap().base_revision(),
                    original_basis.revision
                );
                assert_eq!(panel.compose(1).unwrap(), *snapshot.as_ref().unwrap());
                let painted = compiler
                    .compile(
                        &panel.compose(revision).unwrap(),
                        UiEnvironment::new(
                            UiSurfaceId::new(92).unwrap(),
                            400.0,
                            180.0,
                            1.0,
                            1,
                            revision,
                            1,
                            revision,
                        )
                        .unwrap(),
                        UiTheme::default(),
                    )
                    .unwrap();
                assert!(painted.diagnostics().is_empty());
                assert!(painted.semantics().iter().any(|node| {
                    node.role() == UiSemanticRole::TextInput && node.value() == Some(draft.as_str())
                }));
            })
            .unwrap(),
        );
        assert_eq!(
            host.inspect(&application, token.as_ref().unwrap(), |panel, _| {
                panel.compose(1).unwrap()
            }),
            snapshot
        );
        assert_eq!(current(&application).workspace(), initial.workspace());
        assert!(evidence.finished.borrow().is_empty());
        assert_eq!(fs::read(&source_path).unwrap(), original_bytes);
        assert_navigation(&application);
    }
    let mut ids = Vec::new();
    let batch = host
        .prepare(&application, |_, context| {
            ids.push(
                context
                    .commit_authoring_edit(prepared.take().unwrap())
                    .unwrap(),
            );
            ids.push(
                context
                    .commit_authoring_edit(stale.take().unwrap())
                    .unwrap(),
            );
        })
        .unwrap();
    assert_ne!(ids[0], ids[1]);
    assert_eq!(
        batch.submissions().collect::<Vec<_>>(),
        [(instance, ids[0]), (instance, ids[1])]
    );
    assert_eq!(current(&application).workspace(), initial.workspace());
    assert!(evidence.finished.borrow().is_empty());
    host.submit_and_drive(&mut application, batch, Duration::ZERO);
    {
        let finished = evidence.finished.borrow();
        let [
            (
                accepted_panel,
                accepted_id,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(accepted)),
            ),
            (
                stale_panel,
                stale_id,
                PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(rejected)),
            ),
        ] = finished.as_slice()
        else {
            panic!("both original candidates must receive their exact consuming outcomes");
        };
        assert_eq!((*accepted_panel, *accepted_id), (1, ids[0]));
        assert_eq!((*stale_panel, *stale_id), (1, ids[1]));
        assert!(accepted.applied, "{:?}", accepted.diagnostics);
        assert_eq!(accepted.document, Some(document));
        assert_eq!(accepted.active_document, Some(active));
        assert!(!rejected.applied);
        assert_eq!(rejected.document, Some(document));
        assert_eq!(rejected.active_document, Some(active));
        assert!(rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "tooling.workspace-stale-document-revision"
        }));
    }
    assert_history(&current(&application), document, true, 1, 0);
    assert_leaf(&mut host, &application, &old_read.target, edited);
    assert_navigation(&application);
    let edited_frame = current(&application);
    drop(
        host.prepare(&application, |_, context| {
            assert_eq!(
                context.query_document_authoring(continuation.clone()),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            assert_eq!(
                context.read_document_property(old_read.clone()),
                Err(EditorDocumentQueryRejection::StaleRevision)
            );
            assert!(matches!(
                context
                    .prepare_authoring_property_edit(&original_basis, std::slice::from_ref(&edit)),
                Err(EditorEditPreparationError::QueryBasis(
                    EditorDocumentQueryRejection::StaleRevision,
                ))
            ));
        })
        .unwrap(),
    );
    assert_eq!(current(&application).workspace(), edited_frame.workspace());
    for redo in [false, true] {
        let revision = current(&application)
            .workspace()
            .document(document)
            .unwrap()
            .revision;
        let mutation = if redo {
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
        applied(
            panel_action(
                &mut host,
                &mut application,
                &evidence,
                EditorAction::Mutate(mutation),
            ),
            document,
        );
        assert_leaf(
            &mut host,
            &application,
            &old_read.target,
            if redo { edited } else { fixture::ORIGINAL },
        );
        let frame = current(&application);
        assert_history(
            &frame,
            document,
            redo,
            usize::from(redo),
            usize::from(!redo),
        );
        assert_ne!(
            frame.workspace().document(document).unwrap().revision,
            original_basis.revision
        );
        assert_eq!(
            frame
                .workspace()
                .document(document)
                .unwrap()
                .revision
                .source_id(),
            original_basis.revision.source_id()
        );
        if !redo {
            drop(
                host.prepare(&application, |_, context| {
                    assert_eq!(
                        context.read_document_property(old_read.clone()),
                        Err(EditorDocumentQueryRejection::StaleRevision)
                    );
                    assert!(matches!(
                        context.prepare_authoring_property_edit(
                            &original_basis,
                            std::slice::from_ref(&edit),
                        ),
                        Err(EditorEditPreparationError::QueryBasis(
                            EditorDocumentQueryRejection::StaleRevision,
                        ))
                    ));
                })
                .unwrap(),
            );
            assert_eq!(current(&application).workspace(), frame.workspace());
        }
        assert_eq!(fs::read(&source_path).unwrap(), original_bytes);
        assert_navigation(&application);
    }
    let saving = current(&application)
        .workspace()
        .document(document)
        .unwrap()
        .revision;
    let saved = persist(
        &mut host,
        &mut application,
        &evidence,
        EditorPersistenceAction::Save {
            document,
            base_revision: saving,
        },
    );
    let EditorPersistenceResult::Saved {
        document: saved_document,
        revision: saved_revision,
        digest: saved_digest,
    } = saved
    else {
        panic!("the original panel Save must finish successfully");
    };
    assert_eq!((saved_document, saved_revision), (document, saving));
    assert_history(&current(&application), document, false, 1, 0);
    assert_eq!(
        current(&application)
            .workspace()
            .document(document)
            .unwrap()
            .saved_revision,
        saving
    );
    let saved_bytes = fs::read(&source_path).unwrap();
    assert_ne!(saved_bytes, original_bytes);
    let expected = source(edited);
    match kind {
        EditorDocumentKind::Scene => assert_eq!(
            SceneDocumentCandidate::decode_json_bytes(&saved_bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            expected,
        ),
        EditorDocumentKind::Prefab => assert_eq!(
            PrefabDocumentCandidate::decode_json_bytes(&saved_bytes)
                .unwrap()
                .into_composition_source()
                .unwrap(),
            PrefabDocument::new(expected.placements, expected.entities),
        ),
        EditorDocumentKind::Extension(_) => unreachable!(),
    }
    assert_navigation(&application);

    applied(
        panel_action(
            &mut host,
            &mut application,
            &evidence,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: saving,
            }),
        ),
        document,
    );
    assert_leaf(&mut host, &application, &old_read.target, fixture::ORIGINAL);
    assert_history(&current(&application), document, true, 0, 1);
    assert_eq!(fs::read(&source_path).unwrap(), saved_bytes);
    let mut reopening_basis = None;
    drop(
        host.prepare(&application, |_, context| {
            reopening_basis = Some(context.document_query_basis(document).unwrap());
        })
        .unwrap(),
    );
    let reopening_basis = reopening_basis.unwrap();
    let reopened = persist(
        &mut host,
        &mut application,
        &evidence,
        EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: reopening_basis.revision,
            },
        },
    );
    let EditorPersistenceResult::Opened {
        document: reopened_document,
        revision,
        digest,
    } = reopened
    else {
        panic!("the original application must reopen its saved source");
    };
    assert_eq!(reopened_document, document);
    assert_ne!(revision.source_id(), original_basis.revision.source_id());
    assert_eq!(
        digest, saved_digest,
        "Reopen must admit the same complete canonical source that Save published"
    );
    let reopened_frame = current(&application);
    assert_history(&reopened_frame, document, false, 0, 0);
    assert_eq!(
        reopened_frame
            .workspace()
            .document(document)
            .unwrap()
            .revision,
        revision
    );
    assert_eq!(
        reopened_frame
            .workspace()
            .document(document)
            .unwrap()
            .saved_revision,
        revision
    );
    drop(
        host.prepare(&application, |_, context| {
            assert_eq!(
                context.query_document_authoring(continuation.clone()),
                Err(EditorDocumentQueryRejection::StaleIncarnation)
            );
            assert_eq!(
                context.read_document_property(old_read.clone()),
                Err(EditorDocumentQueryRejection::StaleIncarnation)
            );
            for basis in [&original_basis, &reopening_basis] {
                assert!(matches!(
                    context.prepare_authoring_property_edit(basis, std::slice::from_ref(&edit)),
                    Err(EditorEditPreparationError::QueryBasis(
                        EditorDocumentQueryRejection::StaleIncarnation,
                    ))
                ));
            }
            let (read, _, _) = discover_leaf(context, document, large_map);
            assert_eq!(read.target.property, old_read.target.property);
            assert_eq!(read.target.basis.revision, revision);
            assert_eq!(
                context.read_document_property(read).unwrap().value,
                EditorPropertyValue::Present(ComponentValue::U64(edited))
            );
        })
        .unwrap(),
    );
    assert_eq!(
        current(&application).workspace(),
        reopened_frame.workspace()
    );
    assert_eq!(fs::read(&source_path).unwrap(), saved_bytes);
    assert_navigation(&application);
    assert_eq!(evidence.created.get(), 1);
    assert_eq!(evidence.finished.borrow().len(), 9);
    drop(host);
    assert_eq!(*evidence.dropped.borrow(), [1]);
    assert!(
        application.close(),
        "the original Application must physically finish process-exit custody"
    );
    assert!(application.request_exit().exit_ready());
}
