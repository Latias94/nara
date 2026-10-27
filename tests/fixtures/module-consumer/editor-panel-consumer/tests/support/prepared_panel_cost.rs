//! Bounded, control-first diagnostic, not a benchmark SLA or native latency qualification.
//!
//! Project/schema admission, intent/basis creation, ImGui painting outside the two panel calls,
//! assertions, readback, caller-owned result disposal and cleanup are excluded. Preparation includes
//! each public API's real admission/revalidation. Consuming calls include application drive; the
//! panel path also includes frontend admission, dispatch and exact-result delivery to the panel.
//! Production-internal allocation/destruction remains inside the called operations. Allocations
//! are not measured; this makes no allocation-free, native-latency or speedup claim. Fixed order
//! and readback between samples can affect caches and scheduling. Record the investigation trigger
//! before running; no timing threshold controls test success.

use super::*;

use nara::scene::AuthoringDocumentReadiness;

const WARMUPS: usize = 8;
const SAMPLES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Direct,
    Panel,
}

struct CostPanelState {
    intent: Option<(EditorDocumentQueryBasis, [AuthoringPropertyEdit; 1])>,
    queued: Option<(u128, u128, PanelSubmissionId)>,
    completions: Vec<(PanelSubmissionId, PanelActionDispatch)>,
    paints: usize,
}

struct CostPanel(Rc<RefCell<CostPanelState>>);

impl ImguiEditorPanel for CostPanel {
    fn show(&mut self, _: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let intent = {
            let mut state = self.0.borrow_mut();
            state.paints += 1;
            state.intent.take()
        };
        let Some((captured, edits)) = intent else {
            return;
        };

        let started = Instant::now();
        let prepared = context.prepare_authoring_property_edit(&captured, &edits);
        let prepare_ns = started.elapsed().as_nanos();
        let prepared = prepared.unwrap();
        assert_candidate(&prepared, &captured);

        let started = Instant::now();
        let queued = context.commit_authoring_edit(prepared);
        let enqueue_ns = started.elapsed().as_nanos();
        let submission = queued.unwrap();
        self.0.borrow_mut().queued = Some((prepare_ns, enqueue_ns, submission));
    }

    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        // Retain the real callback result; inspect and dispose of it after the dispatch timer.
        self.0.borrow_mut().completions.push((id, result));
    }
}

fn assert_candidate(prepared: &PreparedEditorDocumentEdit, captured: &EditorDocumentQueryBasis) {
    assert_eq!(prepared.document(), captured.document);
    assert_eq!(prepared.base_revision(), captured.revision);
    assert_eq!(prepared.patch().operations.len(), 1);
    assert_eq!(prepared.inverse().operations.len(), 1);
}

fn assert_applied(
    outcome: &EditorActionOutcome,
    document: EditorDocumentId,
    active: EditorDocumentId,
) {
    let EditorActionOutcome::Workspace(report) = outcome else {
        panic!("the consuming edit did not return its workspace outcome: {outcome:?}");
    };
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(report.document, Some(document));
    assert_eq!(report.active_document, Some(active));
    assert!(report.opened_document.is_none());
}

fn assert_retained_navigation(
    app: &EditorApplication,
    document: EditorDocumentId,
    initial: &EditorFrameModel,
) {
    let current = frame(app);
    assert_eq!(current.workspace().documents.len(), 2);
    assert_eq!(
        current.workspace().active_document,
        initial.workspace().active_document
    );
    assert_ne!(current.workspace().active_document, Some(document));
    assert_eq!(
        current.scene_preview_document(),
        initial.scene_preview_document()
    );
    assert_eq!(
        current.workspace().active_scene.as_ref().unwrap().selection,
        initial.workspace().active_scene.as_ref().unwrap().selection
    );
    let tab = current.workspace().document(document).unwrap();
    let original = initial.workspace().document(document).unwrap();
    assert_eq!(tab.source_binding, original.source_binding);
    assert_eq!(tab.authoring_schema_basis, original.authoring_schema_basis);
    assert_eq!(tab.revision.source_id(), original.revision.source_id());
    assert_eq!(tab.saved_revision, original.saved_revision);
    assert_eq!(
        tab.authoring_readiness,
        Some(AuthoringDocumentReadiness::Degraded)
    );
    for retained in initial
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
}

fn assert_amount(app: &mut EditorApplication, document: EditorDocumentId, expected: u64) {
    let captured = basis(app, document);
    let observed = root_read(
        app,
        EditorPropertyRead {
            target: EditorPropertyTarget {
                basis: captured,
                entity: id("anchor"),
                component: ComponentTypeId::new("consumer.Link"),
                property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            },
            limits: EditorPropertyReadLimits::new(1024, 1, 8).unwrap(),
        },
    )
    .unwrap();
    assert_eq!(
        observed.value,
        EditorPropertyValue::Present(ComponentValue::U64(expected))
    );
}

fn print_samples(label: &str, samples: &mut [u128]) {
    assert_eq!(samples.len(), SAMPLES);
    samples.sort_unstable();
    let p50 = samples[(samples.len() * 50).div_ceil(100) - 1];
    let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
    println!("  {label}: sorted_samples_ns={samples:?} P50_ns={p50} P95_ns={p95} (nearest rank)");
}

fn run_mode(
    kind: EditorDocumentKind,
    extra_entities: usize,
    source: &str,
    contents: &str,
    schema_json: &str,
    mode: Mode,
) {
    let project = text_project::TextProject::new();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        schema_json,
    )
    .unwrap();
    let startup_contents =
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("active"))])
            .to_json_string()
            .unwrap();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        &startup_contents,
    )
    .unwrap();
    fs::write(project.root.join(source), contents).unwrap();
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
        panic!("source Open was not admitted: {opening:?}");
    };
    let request = *request;
    wait(&mut app, |frame| frame.document_open().result().is_some());
    let EditorDocumentOpenResult::Opened {
        request: completed,
        document,
    } = frame(&app).document_open().result().unwrap()
    else {
        panic!("the named diagnostic source did not open");
    };
    assert_eq!(request, completed);
    assert_ne!(document, active);
    let acknowledged = submit(
        &mut app,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen { request }),
    );
    assert!(matches!(acknowledged.actions(), [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request: accepted }
        ))
    ] if *accepted == request));
    for action in [
        EditorNavigationAction::SetActiveDocument { document: active },
        EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(id("active")),
        },
    ] {
        let result = submit(&mut app, EditorAction::Navigate(action));
        assert!(matches!(result.actions(), [
            EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))
        ] if report.applied));
    }
    assert!(frame(&app).document_open().result().is_none());
    assert_amount(&mut app, document, 1);
    let initial = frame(&app);
    let target = initial.workspace().document(document).unwrap();
    assert_eq!(target.kind, kind);
    assert!(target.source_binding.is_some());
    assert!(target.authoring_schema_basis.is_some());
    assert_eq!(target.history.undo_depth, 0);
    assert_eq!(target.history.redo_depth, 0);
    assert_eq!(initial.workspace().active_document, Some(active));
    assert_retained_navigation(&app, document, &initial);

    let shared = Rc::new(RefCell::new(CostPanelState {
        intent: None,
        queued: None,
        completions: Vec::with_capacity(1),
        paints: 0,
    }));
    let mut panel = if mode == Mode::Panel {
        let mut composition = EditorNativeComposition::new();
        let captured = Rc::clone(&shared);
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    "consumer.prepared-panel-cost",
                    "Prepared panel cost",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Floating,
                    true,
                ),
                move || CostPanel(Rc::clone(&captured)),
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
        // Install and paint the real registered instance before the eight edit warmups.
        for _ in 0..4 {
            let draw = context.begin_frame();
            let candidate = frontend.show(draw.ui(), &mut app).unwrap();
            drop(draw);
            if let Some(candidate) = candidate {
                frontend.submit_and_drive(&mut app, candidate, Duration::ZERO);
            }
            if shared.borrow().paints != 0 {
                break;
            }
        }
        assert_ne!(
            shared.borrow().paints,
            0,
            "the registered panel never painted"
        );
        assert_eq!(
            frame(&app).workspace().documents,
            initial.workspace().documents
        );
        Some((frontend, context))
    } else {
        None
    };

    let mut prepare_samples = Vec::with_capacity(SAMPLES);
    let mut enqueue_samples = Vec::with_capacity(SAMPLES);
    let mut consume_samples = Vec::with_capacity(SAMPLES);
    for index in 0..WARMUPS + SAMPLES {
        // Initial amount is one: starting at two makes every accepted edit a real change.
        let value = if index % 2 == 0 { 2 } else { 1 };
        let captured = basis(&app, document);
        let edits = [AuthoringPropertyEdit {
            entity: id("anchor"),
            component: ComponentTypeId::new("consumer.Link"),
            component_version: ComponentSchemaVersion::ONE,
            property: ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            value: ComponentValue::U64(value),
        }];
        let before = frame(&app);
        let (prepare_ns, enqueue_ns, consume_ns) = if let Some((frontend, context)) = panel.as_mut()
        {
            {
                let mut state = shared.borrow_mut();
                assert!(state.intent.is_none());
                assert!(state.queued.is_none());
                assert!(state.completions.is_empty());
                state.intent = Some((captured.clone(), edits));
            }
            let draw = context.begin_frame();
            let candidate = frontend.show(draw.ui(), &mut app).unwrap();
            drop(draw);
            let (prepare_ns, enqueue_ns, submission) =
                shared.borrow_mut().queued.take().expect(
                    "the real panel must synchronously prepare and enqueue exactly one edit",
                );
            assert!(shared.borrow().intent.is_none());
            assert!(shared.borrow().completions.is_empty());
            assert_eq!(
                frame(&app).workspace().documents,
                before.workspace().documents
            );
            let candidate = candidate.expect("typed enqueue must produce a frontend candidate");

            let started = Instant::now();
            let update = frontend.submit_and_drive(&mut app, candidate, Duration::ZERO);
            let consume_ns = started.elapsed().as_nanos();

            assert!(update.actions_observed());
            assert!(!update.exit_ready());
            let (completed, outcome) = {
                let mut state = shared.borrow_mut();
                assert_eq!(
                    state.completions.len(),
                    1,
                    "one exact completion per accepted edit"
                );
                state.completions.pop().unwrap()
            };
            assert_eq!(completed, submission);
            let PanelActionDispatch::Dispatched(outcome) = outcome else {
                panic!("the frontend did not dispatch its typed prepared edit: {outcome:?}");
            };
            assert_applied(&outcome, document, active);
            drop(outcome);
            (prepare_ns, Some(enqueue_ns), consume_ns)
        } else {
            let started = Instant::now();
            let prepared = app.prepare_authoring_property_edit(&captured, &edits);
            let prepare_ns = started.elapsed().as_nanos();
            let prepared = prepared.unwrap();
            assert_candidate(&prepared, &captured);
            assert_eq!(
                frame(&app).workspace().documents,
                before.workspace().documents
            );

            let started = Instant::now();
            let committed = app.commit_authoring_edit(prepared, Duration::ZERO);
            let consume_ns = started.elapsed().as_nanos();

            let [EditorApplicationActionDispatch::Dispatched(outcome)] = committed.actions() else {
                panic!("the direct Application did not dispatch its prepared edit: {committed:?}");
            };
            assert_applied(outcome, document, active);
            assert!(committed.update().actions_observed());
            assert!(!committed.update().exit_ready());
            drop(committed);
            (prepare_ns, None, consume_ns)
        };
        let current = frame(&app);
        let tab = current.workspace().document(document).unwrap();
        assert_ne!(tab.revision, captured.revision);
        assert_eq!(tab.history.undo_depth, index + 1);
        assert_eq!(tab.history.redo_depth, 0);
        assert_amount(&mut app, document, value);
        assert_retained_navigation(&app, document, &initial);
        if index >= WARMUPS {
            prepare_samples.push(prepare_ns);
            if let Some(enqueue_ns) = enqueue_ns {
                enqueue_samples.push(enqueue_ns);
            }
            consume_samples.push(consume_ns);
        }
    }

    // Check the last history boundary in both directions without adding measured work.
    for (redo, value, undo_depth, redo_depth) in [
        (false, 2, WARMUPS + SAMPLES - 1, 1),
        (true, 1, WARMUPS + SAMPLES, 0),
    ] {
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
        let result = submit(&mut app, EditorAction::Mutate(mutation));
        let [EditorApplicationActionDispatch::Dispatched(outcome)] = result.actions() else {
            panic!("history replay did not dispatch: {result:?}");
        };
        assert_applied(outcome, document, active);
        let current = frame(&app);
        let history = current.workspace().document(document).unwrap().history;
        assert_eq!(history.undo_depth, undo_depth);
        assert_eq!(history.redo_depth, redo_depth);
        assert_amount(&mut app, document, value);
        assert_retained_navigation(&app, document, &initial);
    }
    assert_eq!(
        fs::read_to_string(project.root.join(source)).unwrap(),
        contents
    );
    assert_eq!(
        fs::read_to_string(project.root.join("scenes/startup.scene.json")).unwrap(),
        startup_contents
    );
    drop(panel);
    assert!(
        app.close(),
        "the diagnostic project did not physically retire"
    );
    println!(
        "prepared_panel_cost kind={kind:?} extra_entities={extra_entities} entities={} \
         known_Link_records={} unknown_records={} source={source} mode={mode:?} \
         warmups={WARMUPS} samples={SAMPLES}",
        extra_entities + 1,
        extra_entities + 1,
        extra_entities + 1,
    );
    print_samples("prepare", &mut prepare_samples);
    if mode == Mode::Panel {
        print_samples("panel_typed_enqueue", &mut enqueue_samples);
    }
    print_samples(
        if mode == Mode::Direct {
            "direct_commit_drive"
        } else {
            "panel_frontend_dispatch_drive"
        },
        &mut consume_samples,
    );
}

#[test]
#[ignore = "bounded diagnostic; record the investigation trigger before running, not an SLA"]
fn prepared_panel_property_cost_diagnostic() {
    println!(
        "Control-first, fixed order: Scene then Prefab, extra 0 then 256, Direct then Panel. \
         Fresh matched projects; every target record has Link(target=anchor, amount=1) and an unknown component. \
         Timers cover only real public calls; project admission, intent/basis creation, UI work outside those \
         calls, correctness readback, caller-owned result disposal and cleanup are excluded. \
         API revalidation and production-internal allocation/destruction are included. \
         Allocations are not measured (no allocation-free claim); no native latency or speedup claims. \
         Timing thresholds never fail this diagnostic; fixed-order/caching/scheduler effects remain."
    );
    let schemas = bundle();
    let schema_json = schemas.to_json_string().unwrap();
    let link = schemas.owners()[0]
        .current()
        .components
        .iter()
        .find(|schema| schema.id().as_str() == "consumer.Link")
        .unwrap();
    for (kind, source) in [
        (EditorDocumentKind::Scene, "scenes/panel-cost.scene.json"),
        (EditorDocumentKind::Prefab, "prefabs/panel-cost.prefab.json"),
    ] {
        for extra_entities in [0, 256] {
            let entities = std::iter::once(id("anchor"))
                .chain((0..extra_entities).map(|index| id(&format!("populated-{index:04}"))))
                .map(|entity| {
                    SceneEntityRecord::new(entity)
                        .with_component(
                            link.id().clone(),
                            SceneComponentRecord::new(
                                link.version(),
                                link.authoring_initial_value().unwrap().clone(),
                            )
                            .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
                        )
                        .with_component(
                            ComponentTypeId::new("consumer.Unknown"),
                            SceneComponentRecord::new(
                                ComponentSchemaVersion::ONE,
                                ComponentValue::String("retained unknown record".into()),
                            ),
                        )
                })
                .collect::<Vec<_>>();
            let contents = match kind {
                EditorDocumentKind::Scene => SceneDocument::from_ordered_roots(entities)
                    .to_json_string()
                    .unwrap(),
                EditorDocumentKind::Prefab => PrefabDocument::from_ordered_roots(entities)
                    .to_json_string()
                    .unwrap(),
                _ => unreachable!(),
            };
            for mode in [Mode::Direct, Mode::Panel] {
                run_mode(kind, extra_entities, source, &contents, &schema_json, mode);
            }
        }
    }
}
