//! Shared Scene fixture and public Session choreography for the two Snap placements.

use nara::{
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession,
    },
    reflect::{
        AuthoringSchemaBundle, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueKind,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation,
    },
};
use nara_editor_panel_module_consumer::snap_local_position::{
    self as snap, DEFAULT_GRID, SnapPlan, plan_snap_local_position, read_requests,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId,
    EditorDocumentOpenResult, EditorDocumentQueryBasis, EditorPersistenceAction,
    EditorPersistenceAttemptId, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPersistenceView, EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
    EditorPropertyObservation, EditorPropertyValue,
};
use std::{
    fs,
    time::{Duration, Instant},
};
#[path = "text_project.rs"]
#[allow(dead_code)]
mod text_project;
pub const SOURCE: &str = "scenes/snap.scene.json";
pub const TRANSFORM: &str = "nara.transform.Transform2d";

pub fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

pub fn transform(position: [f64; 2]) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "translation",
                ComponentValue::map([
                    ("x", ComponentValue::f64(position[0]).unwrap()),
                    ("y", ComponentValue::f64(position[1]).unwrap()),
                ]),
            ),
            ("rotation", ComponentValue::f64(0.375).unwrap()),
            (
                "scale",
                ComponentValue::map([
                    ("x", ComponentValue::f64(2.0).unwrap()),
                    ("y", ComponentValue::f64(3.0).unwrap()),
                ]),
            ),
        ]),
    )
}

pub fn scene(position: [f64; 2]) -> SceneDocument {
    SceneDocument::new(
        [id("parent"), id("untouched")],
        [
            SceneEntityRecord::new(id("parent"))
                .with_children([id("actor")])
                .with_component(ComponentTypeId::new(TRANSFORM), transform([101.0, -203.0])),
            SceneEntityRecord::new(id("actor"))
                .with_component(ComponentTypeId::new(TRANSFORM), transform(position))
                .with_component(
                    ComponentTypeId::new("nara.scene.Name"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String("Preserved actor".into()),
                    ),
                ),
            SceneEntityRecord::new(id("untouched")),
        ],
    )
}

pub fn fixture(position: [f64; 2]) -> text_project::TextProject {
    let project = text_project::TextProject::new();
    // The existing recipe, not a hand-written Transform schema, owns the bundle.
    let bundle = AuthoringSchemaBundle::from_json_bytes(
        &fs::read(project.root.join("schema/authoring.bundle.json")).unwrap(),
    )
    .unwrap();
    let schema = bundle
        .owners()
        .iter()
        .flat_map(|owner| &owner.current().components)
        .find(|schema| schema.id().as_str() == TRANSFORM)
        .unwrap();
    assert_eq!(schema.version(), ComponentSchemaVersion::ONE);
    for field in ["translation.x", "translation.y"] {
        let field = schema
            .fields()
            .iter()
            .find(|entry| entry.id().as_str() == field)
            .unwrap();
        assert_eq!(field.value_kind(), ComponentValueKind::F64);
    }
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("observer"))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join(SOURCE),
        scene(position).to_json_string().unwrap(),
    )
    .unwrap();
    project
}

pub fn open(project: &text_project::TextProject) -> EditorProjectSession {
    let mut host = EditorProjectOpenHost::new();
    let mut pending = host.begin(project.capability(), None);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.advance(pending) {
            EditorProjectOpenProgress::Opened(session) => return session,
            EditorProjectOpenProgress::Pending(next) => pending = next,
            EditorProjectOpenProgress::Rejected(error) => panic!("{:?}", error.diagnostics()),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn close(session: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(
        session.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn dispatch(session: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = session.current_frame().revision();
    let report = session
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    session.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

pub fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(&outcome, EditorActionOutcome::Workspace(report) if report.applied),
        "{outcome:?}"
    );
}

pub fn wait(session: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(session) {
        assert!(
            Instant::now() < deadline,
            "public Session operation did not settle"
        );
        session.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn open_source(session: &mut EditorProjectSession) -> EditorDocumentId {
    wait(session, |s| {
        !s.current_frame().project_browser().has_pending_work()
    });
    let outcome = dispatch(
        session,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: SOURCE.into(),
        }),
    );
    let EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
        request,
    }) = outcome
    else {
        close(session);
        panic!("{outcome:?}")
    };
    wait(session, |s| {
        s.current_frame().document_open().result().is_some()
    });
    let result = session.current_frame().document_open().result().unwrap();
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = result
    else {
        close(session);
        panic!("source did not open: {result:?}")
    };
    assert_eq!(completed, request);
    assert!(
        matches!(dispatch(session, EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request })),
        EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::OpenAcknowledged { request: acknowledged }) if acknowledged == request)
    );
    document
}

pub fn basis(
    session: &EditorProjectSession,
    document: EditorDocumentId,
) -> EditorDocumentQueryBasis {
    let frame = session.current_frame();
    let tab = frame.workspace().document(document).unwrap();
    EditorDocumentQueryBasis {
        document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    }
}

pub fn observations(
    session: &EditorProjectSession,
    document: EditorDocumentId,
) -> [EditorPropertyObservation; 2] {
    read_requests(&basis(session, document), &id("actor"))
        .unwrap()
        .map(|request| session.read_document_property(request).unwrap())
}

pub fn position(session: &EditorProjectSession, document: EditorDocumentId) -> [f64; 2] {
    observations(session, document).map(|observed| match observed.value {
        EditorPropertyValue::Present(ComponentValue::F64(value)) => value.get(),
        other => panic!("{other:?}"),
    })
}

pub fn proposal(session: &EditorProjectSession, document: EditorDocumentId) -> snap::SnapProposal {
    let [x, y] = observations(session, document);
    let SnapPlan::Change(proposal) = plan_snap_local_position(&x, &y, DEFAULT_GRID).unwrap() else {
        panic!("expected a change")
    };
    proposal
}

pub fn expected_patch(before: bool) -> ScenePatchDocument {
    let mut patch = ScenePatchDocument::new(
        [
            ("translation.x", if before { 23.0 } else { 16.0 }),
            ("translation.y", if before { -25.0 } else { -32.0 }),
        ]
        .into_iter()
        .map(|(field, value)| ScenePatchOperation::SetField {
            entity: id("actor"),
            component: ComponentTypeId::new(TRANSFORM),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new(field),
            value: ComponentValue::f64(value).unwrap(),
        }),
    );
    if before {
        patch.operations.reverse();
    }
    patch
}

pub fn accepted(outcome: EditorActionOutcome) -> EditorPersistenceAttemptId {
    let EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt }) =
        outcome
    else {
        panic!("{outcome:?}")
    };
    attempt
}
pub fn terminal(
    session: &mut EditorProjectSession,
    expected: EditorPersistenceAttemptId,
) -> EditorPersistenceResult {
    wait(
        session,
        |s| matches!(s.current_frame().persistence(), EditorPersistenceView::Terminal { attempt, .. } if attempt == expected),
    );
    let EditorPersistenceView::Terminal { attempt, result } = session.current_frame().persistence()
    else {
        unreachable!()
    };
    assert_eq!(attempt, expected);
    result
}
pub fn acknowledge(session: &mut EditorProjectSession, expected: EditorPersistenceAttemptId) {
    assert!(
        matches!(dispatch(session, EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt: expected })),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { attempt }) if attempt == expected)
    );
}
