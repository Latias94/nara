use super::*;

const CONTROL_ROWS: usize = 257;
const WARMUPS: usize = 8;
const SAMPLES: usize = 64;

#[derive(Debug, Clone, Copy)]
enum Route {
    Direct,
    Panel,
}

#[test]
#[ignore = "release diagnostic"]
fn scene_257_direct() {
    run(EditorDocumentKind::Scene, CONTROL_ROWS, Route::Direct);
}

#[test]
#[ignore = "release diagnostic"]
fn scene_257_panel() {
    run(EditorDocumentKind::Scene, CONTROL_ROWS, Route::Panel);
}

#[test]
#[ignore = "release diagnostic"]
fn scene_large_direct() {
    run(EditorDocumentKind::Scene, KEYED_ROW_COUNT, Route::Direct);
}

#[test]
#[ignore = "release diagnostic"]
fn scene_large_panel() {
    run(EditorDocumentKind::Scene, KEYED_ROW_COUNT, Route::Panel);
}

#[test]
#[ignore = "release diagnostic"]
fn prefab_257_direct() {
    run(EditorDocumentKind::Prefab, CONTROL_ROWS, Route::Direct);
}

#[test]
#[ignore = "release diagnostic"]
fn prefab_257_panel() {
    run(EditorDocumentKind::Prefab, CONTROL_ROWS, Route::Panel);
}

#[test]
#[ignore = "release diagnostic"]
fn prefab_large_direct() {
    run(EditorDocumentKind::Prefab, KEYED_ROW_COUNT, Route::Direct);
}

#[test]
#[ignore = "release diagnostic"]
fn prefab_large_panel() {
    run(EditorDocumentKind::Prefab, KEYED_ROW_COUNT, Route::Panel);
}

struct PreparationSample {
    live_read_ns: u128,
    prepare_ns: u128,
    candidate_read_ns: u128,
    live_scan: (usize, usize),
    candidate_scan: (usize, usize),
}

fn measure_preparation(
    captured: &EditorDocumentQueryBasis,
    prior_note: &str,
    next_note: &str,
    read_live: impl Fn(
        EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection>,
    prepare: impl FnOnce(
        &[AuthoringPropertyEdit],
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError>,
) -> (PreparedEditorDocumentEdit, PreparationSample) {
    let tail = keyed_read(captured, OLD_ROW_KEY, "score");
    let live_request = tail.clone();
    let started = Instant::now();
    let live = read_live(live_request);
    let live_read_ns = started.elapsed().as_nanos();
    let live = live.unwrap();

    let note = note_read(captured);
    let edits = [property_edit(
        &note,
        ComponentValue::String(next_note.into()),
    )];
    let started = Instant::now();
    let prepared = prepare(&edits);
    let prepare_ns = started.elapsed().as_nanos();
    let prepared = prepared.unwrap();

    let preview = prepared.preview();
    let candidate_request = tail.clone();
    let started = Instant::now();
    let candidate = preview.read_property(candidate_request);
    let candidate_read_ns = started.elapsed().as_nanos();
    let candidate = candidate.unwrap();

    assert_selected_scalar(&live, &tail, ComponentValue::U64(ORIGINAL_SCORE));
    assert_selected_scalar(&candidate, &tail, ComponentValue::U64(ORIGINAL_SCORE));
    assert_selected_scalar(
        &read_live(note.clone()).unwrap(),
        &note,
        ComponentValue::String(prior_note.into()),
    );
    assert_selected_scalar(
        &preview.read_property(note.clone()).unwrap(),
        &note,
        ComponentValue::String(next_note.into()),
    );
    assert_eq!(prepared.document(), captured.document);
    assert_eq!(prepared.base_revision(), captured.revision);
    assert_eq!(prepared.patch().operations.len(), 1);
    assert_eq!(prepared.inverse().operations.len(), 1);

    (
        prepared,
        PreparationSample {
            live_read_ns,
            prepare_ns,
            candidate_read_ns,
            live_scan: (live.scanned_entries, live.scanned_logical_bytes),
            candidate_scan: (candidate.scanned_entries, candidate.scanned_logical_bytes),
        },
    )
}

fn assert_applied(
    app: &EditorApplication,
    document: EditorDocumentId,
    report: &EditorWorkspaceCommandReport,
) {
    let current = frame(app);
    let workspace = current.workspace();
    let tab = workspace.document(document).unwrap();
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(report.document, Some(document));
    assert_eq!(report.active_document, workspace.active_document);
    assert_eq!(report.revision, Some(tab.revision));
    assert_eq!(report.dirty, Some(tab.dirty));
    assert!(report.opened_document.is_none());
}

fn print_samples(phase: &str, samples: &[u128]) {
    assert_eq!(samples.len(), SAMPLES);
    let mut ranked = samples.to_vec();
    ranked.sort_unstable();
    let p50 = ranked[(ranked.len() * 50).div_ceil(100) - 1];
    let p95 = ranked[(ranked.len() * 95).div_ceil(100) - 1];
    println!("  {phase}: P50_ns={p50} P95_ns={p95} samples_ns={samples:?}");
}

fn run(kind: EditorDocumentKind, row_count: usize, route: Route) {
    assert_eq!(ORIGINAL_NOTE.len(), UPDATED_NOTE.len());
    assert_ne!(ORIGINAL_NOTE, UPDATED_NOTE);
    let source = match kind {
        EditorDocumentKind::Scene => "scenes/keyed-rows.scene.json",
        EditorDocumentKind::Prefab => "prefabs/keyed-rows.prefab.json",
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    let project = text_project::TextProject::new();
    let (mut app, document, original, startup) = open_nonactive_source(
        &project,
        kind,
        source,
        keyed_rows_schema(),
        [keyed_rows_entity(row_count)],
    );
    let source_path = project.root.join(source);
    let initial = frame(&app);
    let mut panel = match route {
        Route::Direct => None,
        Route::Panel => Some(register_panel(&mut app)),
    };

    let mut live_read_samples = Vec::with_capacity(SAMPLES);
    let mut prepare_samples = Vec::with_capacity(SAMPLES);
    let mut candidate_read_samples = Vec::with_capacity(SAMPLES);
    let mut enqueue_samples = Vec::with_capacity(SAMPLES);
    let mut drive_samples = Vec::with_capacity(SAMPLES);
    let mut scan_work = None;
    for index in 0..WARMUPS + SAMPLES {
        let (prior_note, next_note) = if index % 2 == 0 {
            (ORIGINAL_NOTE, UPDATED_NOTE)
        } else {
            (UPDATED_NOTE, ORIGINAL_NOTE)
        };
        let before = frame(&app);
        let before_tab = before.workspace().document(document).unwrap();
        let (sample, enqueue_ns, drive_ns) = if let Some((host, instance)) = &mut panel {
            let mut queued = None;
            let submission = host
                .prepare(&app, |panel, context| {
                    assert!(context.model().document().is_none());
                    panel.completion = None;
                    let token = context.instance_token();
                    let captured = context.document_query_basis(document).unwrap();
                    let (prepared, sample) = measure_preparation(
                        &captured,
                        prior_note,
                        next_note,
                        |read| context.read_document_property(read),
                        |edits| context.prepare_authoring_property_edit(&captured, edits),
                    );
                    let started = Instant::now();
                    let submitted = context.commit_authoring_edit(prepared);
                    let enqueue_ns = started.elapsed().as_nanos();
                    let submitted = submitted.unwrap();
                    assert!(panel.completion.is_none());
                    assert!(
                        queued
                            .replace((token, submitted, sample, enqueue_ns))
                            .is_none()
                    );
                })
                .unwrap();
            let (token, submitted, sample, enqueue_ns) = queued.unwrap();
            assert_eq!(token.instance(), *instance);
            assert_eq!(submission.submissions().len(), 1);
            assert_eq!(
                submission.submissions().next(),
                Some((*instance, submitted))
            );
            assert_eq!(
                frame(&app).workspace().documents,
                before.workspace().documents
            );

            let started = Instant::now();
            let update = host.submit_and_drive(&mut app, submission, Duration::ZERO);
            let drive_ns = started.elapsed().as_nanos();
            assert!(update.actions_observed());
            assert!(!update.exit_ready());
            host.inspect(&app, &token, |panel, _| {
                assert_applied(&app, document, panel.completed(submitted));
            })
            .unwrap();
            (sample, Some(enqueue_ns), drive_ns)
        } else {
            let captured = app.document_query_basis(document).unwrap();
            let (prepared, sample) = measure_preparation(
                &captured,
                prior_note,
                next_note,
                |read| app.read_document_property(read),
                |edits| app.prepare_authoring_property_edit(&captured, edits),
            );
            assert_eq!(
                frame(&app).workspace().documents,
                before.workspace().documents
            );

            let started = Instant::now();
            let committed = app.commit_authoring_edit(prepared, Duration::ZERO);
            let drive_ns = started.elapsed().as_nanos();
            let [
                EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)),
            ] = committed.actions()
            else {
                panic!("the direct commit must dispatch one workspace result: {committed:?}");
            };
            assert!(committed.update().actions_observed());
            assert!(!committed.update().exit_ready());
            assert_applied(&app, document, report);
            (sample, None, drive_ns)
        };

        let after = frame(&app);
        let after_tab = after.workspace().document(document).unwrap();
        assert_ne!(after_tab.revision, before_tab.revision);
        assert_eq!(
            after_tab.revision.source_id(),
            before_tab.revision.source_id()
        );
        assert_eq!(after_tab.saved_revision, before_tab.saved_revision);
        assert!(after_tab.dirty);
        assert_eq!(
            after_tab.history.undo_depth,
            before_tab.history.undo_depth + 1
        );
        assert_eq!(after_tab.history.redo_depth, 0);
        assert_navigation(&app, &initial, document);
        let note = note_read(&basis(&app, document));
        assert_selected_scalar(
            &app.read_document_property(note.clone()).unwrap(),
            &note,
            ComponentValue::String(next_note.into()),
        );
        let scans = (sample.live_scan, sample.candidate_scan);
        assert_eq!(*scan_work.get_or_insert(scans), scans);
        if index >= WARMUPS {
            live_read_samples.push(sample.live_read_ns);
            prepare_samples.push(sample.prepare_ns);
            candidate_read_samples.push(sample.candidate_read_ns);
            enqueue_samples.extend(enqueue_ns);
            drive_samples.push(drive_ns);
        }
    }

    let base_revision = basis(&app, document).revision;
    let undone = submit(
        &mut app,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision,
        }),
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] =
        undone.actions()
    else {
        panic!("the checkpoint undo must dispatch one workspace result: {undone:?}");
    };
    assert_applied(&app, document, report);
    let checkpoint = frame(&app);
    let tab = checkpoint.workspace().document(document).unwrap();
    assert_eq!(
        (tab.history.undo_depth, tab.history.redo_depth),
        (WARMUPS + SAMPLES - 1, 1)
    );
    assert!(tab.dirty);
    assert_navigation(&app, &initial, document);
    assert!(fs::read(&source_path).unwrap().as_slice() == original.as_bytes());

    let mut expected: serde_json::Value = serde_json::from_str(&original).unwrap();
    let components = &mut expected["payload"]["entities"][0]["components"];
    assert!(components.get("consumer.Unknown").is_some());
    let fields = &mut components[COMPONENT]["value"]["value"];
    assert_eq!(
        fields["wire_rows"]["value"].as_array().unwrap().len(),
        row_count
    );
    assert_eq!(fields["wire_note"]["value"], ORIGINAL_NOTE);
    fields["wire_note"]["value"] = UPDATED_NOTE.into();
    let (saving, digest, saved_bytes) =
        save_exact_source(&mut app, document, &source_path, &expected);
    assert_navigation(&app, &initial, document);

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
    assert_eq!(tab.revision, tab.saved_revision);
    assert!(!tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (0, 0));
    assert_keyed_state(&app, document, OLD_ROW_KEY, ORIGINAL_SCORE, UPDATED_NOTE);
    assert_navigation(&app, &initial, document);
    assert!(fs::read(&source_path).unwrap() == saved_bytes);
    assert_eq!(
        fs::read_to_string(project.root.join(STARTUP)).unwrap(),
        startup
    );
    assert!(
        app.close(),
        "the diagnostic project did not physically retire"
    );

    let ((live_entries, live_bytes), (candidate_entries, candidate_bytes)) = scan_work.unwrap();
    println!(
        "keyed_property_preparation kind={kind:?} route={route:?} rows={row_count} \
         source_bytes={} saved_source_bytes={} warmups={WARMUPS} samples={SAMPLES} \
         live_scanned_entries={live_entries} live_scanned_logical_bytes={live_bytes} \
         candidate_scanned_entries={candidate_entries} candidate_scanned_logical_bytes={candidate_bytes}",
        original.len(),
        saved_bytes.len(),
    );
    print_samples("live_tail_read", &live_read_samples);
    print_samples("prepare", &prepare_samples);
    print_samples("candidate_tail_read", &candidate_read_samples);
    let drive_phase = match route {
        Route::Direct => "direct_commit_drive",
        Route::Panel => {
            print_samples("panel_typed_enqueue", &enqueue_samples);
            "panel_submit_and_drive"
        }
    };
    print_samples(drive_phase, &drive_samples);
}
