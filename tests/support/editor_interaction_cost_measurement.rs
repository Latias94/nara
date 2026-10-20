//! Decision-local public Editor interaction costs, separate from frozen persistence protocols.

use super::*;
use nara::{
    project_host::{EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress},
    reflect::{
        ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        PersistentComponentSchemaOwnerId,
    },
    scene::{AuthoringFieldEdit, SceneComponentRecord},
    tooling::{
        EditorActionBatch, EditorActionBatchReport, EditorActionRejection, EditorNavigationAction,
        EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
        EditorProjectBrowserDiagnosticPhase, EditorProjectBrowserPendingWork,
        EditorProjectBrowserStatus,
    },
};

const WARMUPS: usize = 2;
const SAMPLES: usize = 16;
const TRANSFORM: &str = "nara.transform.Transform2d";

#[test]
#[ignore = "decision-local scalar/Undo/Browser measurement; run explicitly and retain JSON output"]
fn measure_editor_interaction_costs() {
    let _ = built_in_authoring_schema_bundle();
    for population in [64, 1_024, 4_096] {
        let source = transform_scene(population).to_json_string().unwrap();
        measure_scalar_and_undo(population, &source);
        measure_browser_refresh(population);
        measure_browser_scene_open(population, &source);
    }
}

fn measure_scalar_and_undo(population: usize, source: &str) {
    let project = TestProject::new("interaction-scalar");
    fs::write(project.scene_path(), source).unwrap();
    let mut editor = open_project(&project);
    settle_browser(&mut editor);
    let document = editor.current_frame().workspace().active_document.unwrap();
    let selection = editor_driver::dispatch_one(
        &mut editor,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document,
            entity: Some(scene_id("entity-00000")),
        }),
    );
    assert_applied(selection);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(selected_rotation(&editor), 0.0);
    let saved_revision = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .saved_revision;

    for iteration in 0..WARMUPS + SAMPLES {
        let before = editor.current_frame();
        let revision = before.workspace().scene(document).unwrap().revision;
        let edit = AuthoringFieldEdit::single(
            scene_id("entity-00000"),
            ComponentTypeId::new(TRANSFORM),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("rotation"),
            ComponentValue::f64(0.125).unwrap(),
        )
        .unwrap();
        let (outcome, edit_dispatch) = timed_dispatch(
            &mut editor,
            EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
                document,
                base_revision: revision,
                edit,
            }),
        );
        let started = Instant::now();
        let edited = editor.drive_editor_frame(Duration::ZERO);
        let edit_drive = started.elapsed();
        assert_applied(one_outcome(outcome));
        assert_eq!(selected_rotation(&editor), 0.125);
        let scene = edited.workspace().active_scene.as_ref().unwrap();
        assert_ne!(scene.revision, revision);
        assert!(scene.dirty);
        assert_eq!(scene.editor.inspector.entities.len(), population);
        assert_eq!(scene.editor.inspector.history.undo_depth, 1);
        assert_eq!(scene.editor.inspector.history.redo_depth, 0);

        let (outcome, undo_dispatch) = timed_dispatch(
            &mut editor,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: scene.revision,
            }),
        );
        let started = Instant::now();
        let undone = editor.drive_editor_frame(Duration::ZERO);
        let undo_drive = started.elapsed();
        assert_applied(one_outcome(outcome));
        assert_eq!(selected_rotation(&editor), 0.0);
        let scene = undone.workspace().active_scene.as_ref().unwrap();
        assert_ne!(
            scene.revision,
            edited.workspace().active_scene.as_ref().unwrap().revision
        );
        assert!(!scene.dirty);
        assert_eq!(scene.saved_revision, saved_revision);
        assert_eq!(scene.editor.inspector.history.undo_depth, 0);
        assert_eq!(scene.editor.inspector.history.redo_depth, 1);
        assert_eq!(fs::read(project.scene_path()).unwrap(), source.as_bytes());
        if iteration >= WARMUPS {
            for (phase, elapsed) in [
                ("edit-dispatch", edit_dispatch),
                ("edit-drive", edit_drive),
                ("undo-dispatch", undo_dispatch),
                ("undo-drive", undo_drive),
            ] {
                emit(
                    "scalar-edit-undo",
                    population,
                    source.len(),
                    iteration - WARMUPS,
                    phase,
                    elapsed,
                );
            }
        }
    }
    close_project(&mut editor);
}

fn measure_browser_refresh(population: usize) {
    let project = TestProject::new("interaction-browser-refresh");
    for index in 0..population {
        fs::write(
            project.root.join(format!("assets/entry-{index:05}.txt")),
            b"fixture",
        )
        .unwrap();
    }
    let source = fs::read(project.scene_path()).unwrap();
    let mut editor = open_project(&project);
    settle_browser(&mut editor);
    let marker = project.root.join("assets/refresh-marker.txt");
    for iteration in 0..WARMUPS + SAMPLES {
        let present = iteration % 2 == 0;
        if present {
            fs::write(&marker, b"refresh").unwrap();
        } else {
            fs::remove_file(&marker).unwrap();
        }
        let generation = editor
            .current_frame()
            .project_browser()
            .snapshot_generation()
            .unwrap();
        // Start after fixture mutation. Completion includes explicit 1 ms polling, but each
        // drive interval covers only its owner call. No output or semantic oracle runs here.
        let completion_started = Instant::now();
        let (outcome, request) = timed_dispatch(
            &mut editor,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Refresh),
        );
        let mut first_drive = None;
        let mut max_drive = Duration::ZERO;
        let mut enumeration_max = Duration::ZERO;
        let mut revalidation_max = Duration::ZERO;
        let mut publication = None;
        let mut query_max = Duration::ZERO;
        loop {
            let before = editor.current_frame();
            let phase = before
                .project_browser()
                .scan_progress()
                .map(|progress| progress.phase());
            let pending = before.project_browser().pending_work();
            let started = Instant::now();
            let frame = editor.drive_editor_frame(Duration::ZERO);
            let duration = started.elapsed();
            first_drive.get_or_insert(duration);
            max_drive = max_drive.max(duration);
            if publication.is_none()
                && frame.project_browser().snapshot_generation() != Some(generation)
            {
                // This call includes snapshot finalization, safe projection, the first query
                // slice and immutable Editor frame publication; it is not a pure scan timer.
                publication = Some(duration);
            } else if pending == EditorProjectBrowserPendingWork::Query {
                query_max = query_max.max(duration);
            } else if phase == Some(EditorProjectBrowserDiagnosticPhase::LineageRevalidation) {
                revalidation_max = revalidation_max.max(duration);
            } else {
                enumeration_max = enumeration_max.max(duration);
            }
            if publication.is_some() && !frame.project_browser().has_pending_work() {
                break;
            }
            assert!(
                completion_started.elapsed() < ASYNC_DRIVER_TIMEOUT,
                "Browser refresh did not settle"
            );
            thread::sleep(ASYNC_POLL_INTERVAL);
        }
        let completion = completion_started.elapsed();
        assert_eq!(
            one_outcome(outcome),
            EditorActionOutcome::ProjectBrowser(
                EditorProjectBrowserActionOutcome::RefreshRequested
            )
        );
        let frame = editor.current_frame();
        let browser = frame.project_browser();
        assert!(browser.failure().is_none(), "{:?}", browser.failure());
        assert!(matches!(
            browser.status(),
            EditorProjectBrowserStatus::Ready | EditorProjectBrowserStatus::ReadyWithDiagnostics
        ));
        assert!(browser.snapshot_generation().unwrap() > generation);
        assert_eq!(
            browser.entry("assets/refresh-marker.txt").is_some(),
            present
        );
        let indexed = browser
            .snapshot()
            .unwrap()
            .entries()
            .iter()
            .filter(|entry| entry.relative_path().starts_with("assets/entry-"))
            .count();
        assert_eq!(indexed, population);
        assert!(browser.entry("assets/entry-00000.txt").is_some());
        assert!(
            browser
                .entry(&format!("assets/entry-{:05}.txt", population - 1))
                .is_some()
        );
        assert_eq!(fs::read(project.scene_path()).unwrap(), source);
        if iteration >= WARMUPS {
            for (phase, elapsed) in [
                ("request", request),
                ("first-drive", first_drive.unwrap()),
                ("max-drive", max_drive),
                ("enumeration-max-drive", enumeration_max),
                ("revalidation-max-drive", revalidation_max),
                ("publication-drive", publication.unwrap()),
                ("query-max-drive", query_max),
                ("completion", completion),
            ] {
                emit(
                    "browser-refresh",
                    population,
                    source.len(),
                    iteration - WARMUPS,
                    phase,
                    elapsed,
                );
            }
        }
    }
    close_project(&mut editor);
}

fn measure_browser_scene_open(population: usize, source: &str) {
    let project = TestProject::new("interaction-browser-open");
    for path in [
        "scenes/alternate-a.scene.json",
        "scenes/alternate-b.scene.json",
    ] {
        fs::write(project.root.join(path), source).unwrap();
    }
    let mut editor = open_project(&project);
    settle_browser(&mut editor);
    for iteration in 0..WARMUPS + SAMPLES {
        let path = if iteration % 2 == 0 {
            "scenes/alternate-a.scene.json"
        } else {
            "scenes/alternate-b.scene.json"
        };
        let previous = editor.current_frame().workspace().active_document.unwrap();
        let completion_started = Instant::now();
        let (outcome, request) = timed_dispatch(
            &mut editor,
            EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
                relative_path: path.into(),
            }),
        );
        let outcome = one_outcome(outcome);
        assert!(
            matches!(
                outcome,
                EditorActionOutcome::ProjectBrowser(
                    EditorProjectBrowserActionOutcome::Opened { .. }
                        | EditorProjectBrowserActionOutcome::AwaitingClose { .. }
                )
            ),
            "Browser Scene Open was not admitted: {outcome:?}"
        );
        let mut first_drive = None;
        let mut max_drive = Duration::ZERO;
        loop {
            let started = Instant::now();
            let frame = editor.drive_editor_frame(Duration::ZERO);
            let duration = started.elapsed();
            first_drive.get_or_insert(duration);
            max_drive = max_drive.max(duration);
            let opened = frame
                .workspace()
                .active_document
                .and_then(|document| frame.workspace().document(document))
                .and_then(|document| document.source_binding.as_ref())
                .is_some_and(|binding| binding.locator() == path);
            if opened && frame.workspace_intent().phase().is_none() {
                break;
            }
            assert!(
                completion_started.elapsed() < ASYNC_DRIVER_TIMEOUT,
                "Browser Scene Open did not settle: {outcome:?}"
            );
            thread::sleep(ASYNC_POLL_INTERVAL);
        }
        let completion = completion_started.elapsed();
        let frame = editor.current_frame();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        assert_ne!(
            scene.document, previous,
            "alternating sources must admit, not activate a tab"
        );
        assert!(frame.workspace().document(previous).is_none());
        assert_eq!(scene.editor.inspector.entities.len(), population);
        assert!(!scene.dirty);
        assert_eq!(scene.editor.inspector.history.undo_depth, 0);
        assert_eq!(scene.editor.inspector.history.redo_depth, 0);
        assert_eq!(
            fs::read(project.root.join(path)).unwrap(),
            source.as_bytes()
        );
        assert_eq!(
            frame.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: previous },
            })
        );
        // Each ordinary replacement has a terminal CloseDocument result. Acknowledge it
        // outside the measurement before requesting the next replacement, as the public
        // workspace-intent contract requires.
        assert!(editor_driver::acknowledge_workspace_intent_result(
            &mut editor
        ));
        let acknowledged = editor.drive_editor_frame(Duration::ZERO);
        assert!(acknowledged.workspace_intent().result().is_none());
        if iteration >= WARMUPS {
            for (phase, elapsed) in [
                ("request", request),
                ("first-drive", first_drive.unwrap()),
                ("max-drive", max_drive),
                ("completion", completion),
            ] {
                emit(
                    "browser-scene-open",
                    population,
                    source.len(),
                    iteration - WARMUPS,
                    phase,
                    elapsed,
                );
            }
        }
    }
    close_project(&mut editor);
}

fn timed_dispatch(
    editor: &mut EditorProjectSession,
    action: EditorAction,
) -> (
    Result<EditorActionBatchReport, EditorActionRejection>,
    Duration,
) {
    // Construct frame-bound commands outside the owner-call timer. The caller inspects the
    // returned report outside that timer; Open checks admission before polling completion.
    let batch = EditorActionBatch::new(editor.current_frame().revision(), [action]);
    let started = Instant::now();
    let result = editor.dispatch_editor_actions(batch);
    let elapsed = started.elapsed();
    (result, elapsed)
}

fn one_outcome(
    result: Result<EditorActionBatchReport, EditorActionRejection>,
) -> EditorActionOutcome {
    let mut outcomes = result.unwrap().into_outcomes();
    assert_eq!(outcomes.len(), 1);
    outcomes.remove(0)
}

fn assert_applied(outcome: EditorActionOutcome) {
    assert!(matches!(outcome, EditorActionOutcome::Workspace(report) if report.applied));
}

fn selected_rotation(editor: &EditorProjectSession) -> f64 {
    let frame = editor.current_frame();
    let selected = frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let component = selected
        .components
        .iter()
        .find(|component| component.component.as_str() == TRANSFORM)
        .unwrap();
    component
        .fields
        .iter()
        .find(|field| field.id.as_str() == "rotation")
        .unwrap()
        .value
        .as_ref()
        .unwrap()
        .as_f64()
        .unwrap()
}

fn transform_scene(population: usize) -> SceneDocument {
    let value = ComponentValue::map([
        (
            "translation",
            ComponentValue::map([
                ("x", ComponentValue::f64(0.0).unwrap()),
                ("y", ComponentValue::f64(0.0).unwrap()),
            ]),
        ),
        ("rotation", ComponentValue::f64(0.0).unwrap()),
        (
            "scale",
            ComponentValue::map([
                ("x", ComponentValue::f64(1.0).unwrap()),
                ("y", ComponentValue::f64(1.0).unwrap()),
            ]),
        ),
    ]);
    SceneDocument::from_ordered_roots((0..population).map(|index| {
        SceneEntityRecord::new(scene_id(&format!("entity-{index:05}"))).with_component(
            ComponentTypeId::new(TRANSFORM),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone()).with_owner(
                PersistentComponentSchemaOwnerId::try_new("nara.transform.components").unwrap(),
            ),
        )
    }))
}

fn open_project(project: &TestProject) -> EditorProjectSession {
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(project.capability(), None);
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Opened(editor) => return editor,
            EditorProjectOpenProgress::Pending(next) => attempt = next,
            EditorProjectOpenProgress::Rejected(error) => {
                panic!("fixture Open rejected: {error:?}")
            }
        }
        assert!(Instant::now() < deadline, "fixture Open did not settle");
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn settle_browser(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if !frame.project_browser().has_pending_work() {
            assert!(frame.project_browser().snapshot().is_some());
            assert!(frame.project_browser().failure().is_none());
            return;
        }
        assert!(Instant::now() < deadline, "fixture Browser did not settle");
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn close_project(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(Instant::now() < deadline, "fixture close did not settle");
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn emit(
    scenario: &str,
    population: usize,
    source_bytes: usize,
    sample: usize,
    phase: &str,
    elapsed: Duration,
) {
    println!(
        "{}",
        serde_json::json!({
            "measurement": "editor-interaction-cost-v1",
            "scenario": scenario,
            "population": population,
            "source_bytes": source_bytes,
            "sample": sample,
            "phase": phase,
            "elapsed_ns": elapsed.as_nanos(),
            "optimized": !cfg!(debug_assertions),
            "poll_interval_ns": ASYNC_POLL_INTERVAL.as_nanos(),
        })
    );
}
