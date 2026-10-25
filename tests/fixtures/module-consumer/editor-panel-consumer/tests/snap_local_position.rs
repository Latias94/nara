//! Independent source-local Snap workflows through public Session and panel APIs.

use std::{
    fs,
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentPropertyStep, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue,
    },
    scene::SceneDocumentCandidate,
};
use nara_editor_panel_module_consumer::snap_local_position::{
    self as snap, DEFAULT_GRID, SnapPlan, SnapRejection, plan_snap_local_position, read_requests,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
    EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPersistenceView, EditorPropertyValue, EditorReopenTarget,
};

#[path = "support/snap_project.rs"]
#[allow(dead_code)]
mod snap_project;
use snap_project::*;

#[test]
fn numeric_policy_has_no_quotient_or_coordinate_cap_and_no_hidden_f32_roundtrip() {
    let cases = [
        ([23.0, -25.0], 16.0, [16.0, -32.0]),
        ([24.0, -24.0], 16.0, [32.0, -32.0]),
        ([8.0, -8.0], 16.0, [16.0, -16.0]),
        ([16.0, -32.0], 16.0, [16.0, -32.0]),
        ([-0.0, 0.0], 16.0, [0.0, 0.0]),
        ([3_000_007.0, 0.0], 16.0, [3_000_000.0, 0.0]),
        ([1.0, 0.0], f64::from_bits(1), [1.0, 0.0]),
        ([1.0, -1.0], f64::MAX, [0.0, 0.0]),
        (
            [0.26, -0.26],
            0.1,
            [0.30000000000000004, -0.30000000000000004],
        ),
    ];
    assert_eq!(DEFAULT_GRID, 16.0);
    for (input, grid, expected) in cases {
        for (input, expected) in [
            (input, expected),
            ([input[1], input[0]], [expected[1], expected[0]]),
        ] {
            let actual = snap::snap_local_position(input, grid).unwrap();
            assert_eq!(actual, expected, "{input:?}, {grid}");
            assert_eq!(snap::snap_local_position(actual, grid).unwrap(), actual);
        }
    }
    let below = f64::from_bits(24.0f64.to_bits() - 1);
    let above = f64::from_bits(24.0f64.to_bits() + 1);
    assert_eq!(
        snap::snap_local_position([below, -below], 16.0).unwrap(),
        [16.0, -16.0]
    );
    assert_eq!(
        snap::snap_local_position([above, -above], 16.0).unwrap(),
        [32.0, -32.0]
    );
    for grid in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            snap::snap_local_position([1.0, 2.0], grid),
            Err(SnapRejection::InvalidGrid)
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for input in [[value, 1.0], [1.0, value]] {
            assert_eq!(
                snap::snap_local_position(input, 16.0),
                Err(SnapRejection::NonFinitePosition)
            );
        }
    }
    let max = f64::from(f32::MAX);
    for input in [[max * 2.0, 1.0], [1.0, -max * 2.0]] {
        assert_eq!(
            snap::snap_local_position(input, 16.0),
            Err(SnapRejection::PositionOutOfRange)
        );
    }
    for input in [[max, 1.0], [1.0, -max]] {
        assert_eq!(
            snap::snap_local_position(input, 2.0f64.powi(105)),
            Err(SnapRejection::ResultOutOfRange)
        );
    }
}

#[test]
fn observations_require_exact_dotted_roots_and_coherent_basis_and_present_scalars() {
    let project = fixture([23.0, -25.0]);
    let mut session = open(&project);
    let document = open_source(&mut session);
    let before = session.current_frame();
    let [x, y] = observations(&session, document);
    for (request, field) in read_requests(&basis(&session, document), &id("actor"))
        .unwrap()
        .into_iter()
        .zip(["translation.x", "translation.y"])
    {
        assert_eq!(
            request.target.property,
            ComponentPropertyPath::new(ComponentFieldId::new(field))
        );
        assert_eq!(request.limits.max_value_nodes(), 1);
        assert_eq!(request.limits.max_logical_bytes(), 8);
        assert_eq!(request.limits.max_scan_entries(), 64);
        assert_eq!(request.limits.max_scan_logical_bytes(), 4096);
        assert_eq!(
            request.limits.max_metadata_bytes(),
            "actor".len() + TRANSFORM.len() + field.len()
        );
    }
    for observed in [&x, &y] {
        assert_eq!(observed.value_nodes, 1);
        assert_eq!(observed.value_logical_bytes, 8);
        assert!(observed.scanned_entries <= 64 && observed.scanned_logical_bytes <= 4096);
    }
    assert_eq!(
        plan_snap_local_position(&y, &x, 16.0),
        Err(SnapRejection::ObservationMismatch)
    );
    let mut variants = Vec::new();
    let mut wrong = y.clone();
    wrong.target.entity = id("parent");
    variants.push(wrong);
    let mut wrong = y.clone();
    wrong.target.component = ComponentTypeId::new("nara.scene.Name");
    variants.push(wrong);
    let mut wrong = y.clone();
    wrong.target.basis.schema = None;
    variants.push(wrong);
    let mut wrong = y.clone();
    wrong
        .target
        .property
        .steps
        .push(ComponentPropertyStep::Member(ComponentFieldId::new("x")));
    variants.push(wrong);
    for wrong in variants {
        assert_eq!(
            plan_snap_local_position(&x, &wrong, 16.0),
            Err(SnapRejection::ObservationMismatch)
        );
    }
    for value in [
        EditorPropertyValue::Missing,
        EditorPropertyValue::Defaulted(ComponentValue::f64(1.0).unwrap()),
        EditorPropertyValue::Present(ComponentValue::U64(1)),
    ] {
        let mut wrong = y.clone();
        wrong.value = value;
        assert_eq!(
            plan_snap_local_position(&x, &wrong, 16.0),
            Err(SnapRejection::ValueUnavailable)
        );
    }
    let mut missing = read_requests(&basis(&session, document), &id("untouched"))
        .unwrap()
        .into_iter();
    assert!(
        session
            .read_document_property(missing.next().unwrap())
            .is_err()
    );
    assert_eq!(
        plan_snap_local_position(&x, &y, 0.0),
        Err(SnapRejection::InvalidGrid)
    );
    let edits = proposal(&session, document).property_edits();
    assert_eq!(edits.len(), 2);
    assert!(
        edits
            .iter()
            .all(|edit| edit.component_version == ComponentSchemaVersion::ONE)
    );
    assert_eq!(session.current_frame().workspace(), before.workspace());
    close(&mut session);
}

#[test]
fn window_free_session_snaps_an_inactive_source_with_one_undo_and_exact_save_reopen() {
    // No Application, ImGui Context, window, GPU or Play runtime is constructed in this workflow.
    let project = fixture([23.0, -25.0]);
    let original = fs::read(project.root.join(SOURCE)).unwrap();
    let mut session = open(&project);
    let active = session.current_frame().workspace().active_document.unwrap();
    applied(dispatch(
        &mut session,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: active,
            entity: Some(id("observer")),
        }),
    ));
    let document = open_source(&mut session);
    applied(dispatch(
        &mut session,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    ));
    let before = session.current_frame();
    let intent = proposal(&session, document);
    assert_eq!(intent.before(), [23.0, -25.0]);
    assert_eq!(intent.after(), [16.0, -32.0]);
    assert_eq!(intent.grid(), DEFAULT_GRID);
    session
        .validate_authoring_query_basis(intent.basis())
        .unwrap();
    let prepare = || {
        session
            .prepare_authoring_property_edit(
                document,
                intent.basis().revision,
                &intent.property_edits(),
            )
            .unwrap()
    };
    let cancelled = prepare();
    assert_eq!(cancelled.patch(), &expected_patch(false));
    assert_eq!(cancelled.inverse(), &expected_patch(true));
    drop(cancelled);
    let prepared = prepare();
    let stale = prepare();
    for _ in 0..2 {
        session.drive_editor_frame(Duration::ZERO);
    }
    assert_eq!(session.current_frame().workspace(), before.workspace());
    assert_eq!(fs::read(project.root.join(SOURCE)).unwrap(), original);
    let commit = session.commit_authoring_edit(prepared);
    applied(commit.clone());
    session.drive_editor_frame(Duration::ZERO);
    let after = session.current_frame();
    assert_eq!(position(&session, document), [16.0, -32.0]);
    assert_eq!(
        after
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    assert_eq!(after.workspace().active_document, Some(active));
    assert_eq!(
        after.workspace().active_scene,
        before.workspace().active_scene
    );
    assert_eq!(
        after.workspace().document(active),
        before.workspace().document(active)
    );
    let rejected = session.commit_authoring_edit(stale);
    assert!(
        matches!(rejected, EditorActionOutcome::Workspace(ref report) if !report.applied),
        "{rejected:?}"
    );
    session.drive_editor_frame(Duration::ZERO);
    assert_eq!(session.current_frame().workspace(), after.workspace());
    assert!(
        session
            .validate_authoring_query_basis(intent.basis())
            .is_err()
    );
    for (redo, expected) in [(false, [23.0, -25.0]), (true, [16.0, -32.0])] {
        let base_revision = basis(&session, document).revision;
        applied(dispatch(
            &mut session,
            EditorAction::Mutate(if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                }
            }),
        ));
        assert_eq!(position(&session, document), expected);
    }
    let no_change_frame = session.current_frame();
    let [x, y] = observations(&session, document);
    assert_eq!(
        plan_snap_local_position(&x, &y, 16.0),
        Ok(SnapPlan::NoChange)
    );
    session.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        session.current_frame().workspace(),
        no_change_frame.workspace()
    );
    assert_eq!(fs::read(project.root.join(SOURCE)).unwrap(), original);
    let saving = basis(&session, document).revision;
    let save = accepted(dispatch(
        &mut session,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: saving,
        }),
    ));
    let saved = terminal(&mut session, save);
    assert!(
        matches!(saved, EditorPersistenceResult::Saved { document: d, revision, .. } if d == document && revision == saving)
    );
    let disk =
        SceneDocumentCandidate::decode_json_bytes(&fs::read(project.root.join(SOURCE)).unwrap())
            .unwrap()
            .to_json_string()
            .unwrap();
    assert_eq!(disk, scene([16.0, -32.0]).to_json_string().unwrap());
    acknowledge(&mut session, save);
    // An old candidate/observation cannot acquire the incarnation produced by Reopen.
    let old_reads = read_requests(&basis(&session, document), &id("actor")).unwrap();
    let mut later_edits = intent.property_edits();
    later_edits[0].value = ComponentValue::f64(48.0).unwrap();
    let changed = session
        .prepare_authoring_property_edit(document, saving, &later_edits)
        .unwrap();
    let reopen = accepted(dispatch(
        &mut session,
        EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: saving,
            },
        }),
    ));
    assert_ne!(save, reopen);
    let reopened = terminal(&mut session, reopen);
    let EditorPersistenceResult::Opened {
        document: reopened_document,
        revision: reopened_revision,
        ..
    } = reopened
    else {
        panic!("{reopened:?}")
    };
    assert_eq!(reopened_document, document);
    assert_ne!(reopened_revision, saving);
    // A late observer cannot dismiss Reopen using Save's already acknowledged attempt.
    assert!(matches!(
        dispatch(
            &mut session,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt: save })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Refused(
            nara_tooling::EditorPersistenceRejection::ResultMismatch
        ))
    ));
    assert_eq!(
        session.current_frame().persistence(),
        EditorPersistenceView::Terminal {
            attempt: reopen,
            result: reopened
        }
    );
    acknowledge(&mut session, reopen);
    assert_eq!(
        session.read_document_property(old_reads[0].clone()),
        Err(EditorDocumentQueryRejection::StaleIncarnation)
    );
    let rejection = session.commit_authoring_edit(changed);
    assert!(matches!(rejection, EditorActionOutcome::Workspace(ref report) if !report.applied));
    session.drive_editor_frame(Duration::ZERO);
    assert_eq!(position(&session, document), [16.0, -32.0]);
    let current = session.current_frame();
    let tab = current.workspace().document(document).unwrap();
    assert_eq!(tab.revision, reopened_revision);
    assert_ne!(tab.revision.source_id(), saving.source_id());
    assert!(!tab.dirty);
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 0);
    assert_eq!(
        current.workspace().active_scene,
        before.workspace().active_scene
    );
    assert_eq!(
        current.workspace().document(active),
        before.workspace().document(active)
    );
    applied(commit); // The retained originator result survives unrelated later observations.
    close(&mut session);
}

#[test]
fn stale_reads_refuse_preparation_and_no_change_does_not_create_a_candidate() {
    let project = fixture([-0.0, 32.0]);
    let mut session = open(&project);
    let document = open_source(&mut session);
    let before = session.current_frame();
    let [x, y] = observations(&session, document);
    assert_eq!(
        plan_snap_local_position(&x, &y, 16.0),
        Ok(SnapPlan::NoChange)
    );
    assert_eq!(session.current_frame().workspace(), before.workspace());
    let captured = basis(&session, document);
    let edits = [nara::scene::AuthoringPropertyEdit {
        entity: id("actor"),
        component: ComponentTypeId::new(TRANSFORM),
        component_version: ComponentSchemaVersion::ONE,
        property: ComponentPropertyPath::new(ComponentFieldId::new("translation.x")),
        value: ComponentValue::f64(23.0).unwrap(),
    }];
    let prepared = session
        .prepare_authoring_property_edit(document, captured.revision, &edits)
        .unwrap();
    applied(session.commit_authoring_edit(prepared));
    session.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        session.validate_authoring_query_basis(&captured),
        Err(nara::project_host::EditorEditPreparationError::QueryBasis(
            EditorDocumentQueryRejection::StaleRevision
        ))
    ));
    let intent = proposal(&session, document);
    assert_eq!(intent.property_edits().len(), 1);
    assert_eq!(
        intent.property_edits()[0].property.root.as_str(),
        "translation.x"
    );
    let mut wrong = y;
    wrong.target.basis = basis(&session, document);
    assert_eq!(
        plan_snap_local_position(&x, &wrong, 16.0),
        Err(SnapRejection::ObservationMismatch)
    );
    close(&mut session);
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod panel {
    use super::*;
    use nara_editor::{
        EditorApplication, EditorLaunchOptions, EditorNativeComposition, ImguiEditorPanel,
        ImguiEditorPanelContext, PanelActionDispatch, PanelSubmissionId,
        workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
    };
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    #[test]
    fn public_snap_installer_opens_and_paints_its_actual_contribution() {
        let project = fixture([23.0, -25.0]);
        let mut composition = EditorNativeComposition::new();
        snap::install(&mut composition).unwrap();
        let (mut app, mut frontend) = composition
            .build(EditorLaunchOptions::new(&project.root))
            .unwrap();
        app.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.current_frame().is_none() {
            assert!(Instant::now() < deadline);
            app.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        let descriptor = app
            .panel_workspace()
            .registry()
            .descriptor(snap::PANEL_KIND)
            .unwrap();
        assert_eq!(descriptor.title(), "Snap Local Position");
        assert_eq!(descriptor.multiplicity(), EditorPanelMultiplicity::Single);
        assert_eq!(descriptor.default_placement(), EditorPanelPlacement::Right);
        assert!(descriptor.initially_open());
        let instances: Vec<_> = app
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|instance| instance.kind() == snap::PANEL_KIND)
            .collect();
        assert_eq!(instances.len(), 1);
        assert!(instances[0].is_open() && instances[0].is_visible());
        let obscuring: Vec<_> = app
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|instance| {
                instance.placement() == EditorPanelPlacement::Right
                    && instance.kind() != snap::PANEL_KIND
            })
            .map(|instance| instance.id())
            .collect();
        for panel in obscuring {
            app.panel_workspace_mut().layout_mut().close(panel);
        }
        let before = app.current_frame().unwrap();
        let mut imgui = dear_imgui_rs::Context::create();
        imgui.set_ini_filename::<PathBuf>(None).unwrap();
        imgui.io_mut().set_display_size([1600.0, 1000.0]);
        imgui.io_mut().set_delta_time(1.0 / 60.0);
        imgui
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        imgui
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        for _ in 0..3 {
            let draw = imgui.begin_frame();
            let queued = frontend.show(draw.ui(), &mut app).unwrap();
            drop(draw);
            if let Some(queued) = queued {
                frontend.submit_and_drive(&mut app, queued, Duration::ZERO);
            }
        }
        assert_eq!(app.current_frame().unwrap().workspace(), before.workspace());
        let mut ready = app.request_exit().exit_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready {
            assert!(Instant::now() < deadline);
            ready = if let Some(frame) = app.current_frame() {
                app.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
                    .update()
                    .exit_ready()
            } else {
                app.advance_project_open().exit_ready()
            };
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    // This spy delegates every operation and callback to the actual contributed SnapPanel.
    // Its only UI probe is the last item's public rectangle; it does not prepare or commit.
    #[derive(Default)]
    struct Evidence {
        queued: Vec<PanelSubmissionId>,
        finished: Vec<(PanelSubmissionId, PanelActionDispatch)>,
    }
    struct Spy {
        panel: Rc<RefCell<snap::SnapPanel>>,
        buttons: Rc<RefCell<[[f32; 2]; 3]>>,
        evidence: Rc<RefCell<Evidence>>,
    }
    impl ImguiEditorPanel for Spy {
        fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
            self.panel.borrow_mut().show(ui, context);
            if let Some(id) = self.panel.borrow().pending() {
                let mut evidence = self.evidence.borrow_mut();
                if evidence.queued.last() != Some(&id) {
                    evidence.queued.push(id);
                }
            }
            let min = ui.item_rect_min();
            let max = ui.item_rect_max();
            let cancel = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
            let stride = max[0] - min[0] + ui.clone_style().item_spacing()[0];
            *self.buttons.borrow_mut() = [
                [cancel[0] - 2.0 * stride, cancel[1]],
                [cancel[0] - stride, cancel[1]],
                cancel,
            ];
        }
        fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
            self.evidence
                .borrow_mut()
                .finished
                .push((id, result.clone()));
            self.panel.borrow_mut().action_finished(id, result);
        }
    }

    #[test]
    fn actual_snap_panel_previews_cancels_and_consumes_its_candidate_with_exact_callback() {
        let project = fixture([23.0, -25.0]);
        // Establish the same independent Session oracle before constructing any native frontend.
        let mut session = open(&project);
        let oracle_document = open_source(&mut session);
        let intent = proposal(&session, oracle_document);
        session
            .validate_authoring_query_basis(intent.basis())
            .unwrap();
        let oracle = session
            .prepare_authoring_property_edit(
                oracle_document,
                intent.basis().revision,
                &intent.property_edits(),
            )
            .unwrap();
        let patch = oracle.patch().clone();
        let inverse = oracle.inverse().clone();
        drop(oracle);
        close(&mut session);

        // Put the real Transform scene at startup for this active-document frontend.
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            scene([23.0, -25.0]).to_json_string().unwrap(),
        )
        .unwrap();
        let mut app = EditorApplication::new(EditorLaunchOptions::new(&project.root));
        app.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.current_frame().is_none() {
            assert!(Instant::now() < deadline);
            app.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = app.current_frame().unwrap();
        let document = frame.workspace().active_document.unwrap();
        app.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: Some(id("actor")),
                    },
                )],
            ),
            Duration::ZERO,
        );
        let before = app.current_frame().unwrap();
        let bytes = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
        let mut registration = EditorNativeComposition::new();
        snap::install(&mut registration).unwrap();
        let state = Rc::new(RefCell::new(snap::SnapPanel::default()));
        let buttons = Rc::new(RefCell::new([[0.0; 2]; 3]));
        let evidence = Rc::new(RefCell::new(Evidence::default()));
        let mut composition = EditorNativeComposition::new();
        let (shared, controls, observed) =
            (Rc::clone(&state), Rc::clone(&buttons), Rc::clone(&evidence));
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    snap::PANEL_KIND,
                    "Snap Local Position",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Right,
                    true,
                ),
                move || Spy {
                    panel: Rc::clone(&shared),
                    buttons: Rc::clone(&controls),
                    evidence: Rc::clone(&observed),
                },
            )
            .unwrap();
        let mut frontend = composition.install_panels(&mut app).unwrap();
        // Keep the real Right-side contribution visible rather than hidden behind Inspector.
        let obscuring: Vec<_> = app
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|panel| {
                panel.placement() == EditorPanelPlacement::Right && panel.kind() != snap::PANEL_KIND
            })
            .map(|panel| panel.id())
            .collect();
        for panel in obscuring {
            app.panel_workspace_mut().layout_mut().close(panel);
        }
        let mut imgui = dear_imgui_rs::Context::create();
        imgui.set_ini_filename::<PathBuf>(None).unwrap();
        imgui.io_mut().set_display_size([1600.0, 1000.0]);
        imgui.io_mut().set_delta_time(1.0 / 60.0);
        imgui
            .io_mut()
            .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
        imgui
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut paint = |imgui: &mut dear_imgui_rs::Context, app: &mut EditorApplication| {
            let draw = imgui.begin_frame();
            let queued = frontend.show(draw.ui(), app).unwrap();
            drop(draw);
            if let Some(queued) = queued {
                frontend.submit_and_drive(app, queued, Duration::ZERO);
            }
        };
        for _ in 0..3 {
            paint(&mut imgui, &mut app);
        }
        let mut click =
            |button: usize, imgui: &mut dear_imgui_rs::Context, app: &mut EditorApplication| {
                // Let changed preview/status layout settle before measuring the real button row.
                for _ in 0..2 {
                    paint(imgui, app);
                }
                imgui.io_mut().add_mouse_pos_event(buttons.borrow()[button]);
                paint(imgui, app);
                imgui
                    .io_mut()
                    .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, true);
                paint(imgui, app);
                imgui
                    .io_mut()
                    .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, false);
                paint(imgui, app);
            };
        click(0, &mut imgui, &mut app);
        {
            let panel = state.borrow();
            let (proposed, candidate) = panel
                .preview()
                .expect("real Preview button prepared the candidate");
            assert_eq!(proposed.after(), intent.after());
            assert_eq!(candidate.patch(), &patch);
            assert_eq!(candidate.inverse(), &inverse);
        }
        click(2, &mut imgui, &mut app);
        assert!(state.borrow().preview().is_none());
        assert_eq!(app.current_frame().unwrap().workspace(), before.workspace());
        assert_eq!(
            fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
            bytes
        );
        click(0, &mut imgui, &mut app);
        click(1, &mut imgui, &mut app);
        let retained = state
            .borrow()
            .completion()
            .cloned()
            .expect("the exact originator callback was retained");
        assert_eq!(evidence.borrow().queued, [retained.0]);
        assert_eq!(evidence.borrow().finished, [retained.clone()]);
        assert!(
            matches!(&retained.1, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
        );
        assert!(state.borrow().pending().is_none());
        assert!(state.borrow().preview().is_none());
        assert_eq!(
            app.current_frame()
                .unwrap()
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            1
        );
        // A second observer drives the Application; it cannot replace Snap's owned completion.
        let current = app.current_frame().unwrap();
        app.submit_and_drive(
            EditorActionBatch::new(current.revision(), []),
            Duration::ZERO,
        );
        assert_eq!(state.borrow().completion(), Some(&retained));
        let current = app.current_frame().unwrap();
        let tab = current.workspace().document(document).unwrap();
        let captured = EditorDocumentQueryBasis {
            document,
            revision: tab.revision,
            schema: tab.authoring_schema_basis.clone(),
        };
        for (read, expected) in read_requests(&captured, &id("actor"))
            .unwrap()
            .into_iter()
            .zip([16.0, -32.0])
        {
            let revision = app.current_frame().unwrap().revision();
            let readback = app.submit_and_drive(
                EditorActionBatch::new(revision, [EditorAction::ReadDocumentProperty(read)]),
                Duration::ZERO,
            );
            let [
                nara_editor::EditorApplicationActionDispatch::Dispatched(
                    EditorActionOutcome::DocumentProperty(Ok(observed)),
                ),
            ] = readback.actions()
            else {
                panic!("{readback:?}")
            };
            assert_eq!(
                observed.value,
                EditorPropertyValue::Present(ComponentValue::f64(expected).unwrap())
            );
        }
        assert_eq!(
            fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
            bytes
        );
        // Prepare against undone source, then let another public observer Redo before Apply.
        let frame = app.current_frame().unwrap();
        let base_revision = frame.workspace().document(document).unwrap().revision;
        let undone = app.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                })],
            ),
            Duration::ZERO,
        );
        assert!(
            matches!(undone.actions(), [nara_editor::EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        );
        click(0, &mut imgui, &mut app);
        assert_eq!(state.borrow().preview().unwrap().1.patch(), &patch);
        let frame = app.current_frame().unwrap();
        let base_revision = frame.workspace().document(document).unwrap().revision;
        let redone = app.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                })],
            ),
            Duration::ZERO,
        );
        assert!(
            matches!(redone.actions(), [nara_editor::EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        );
        let before_stale = app.current_frame().unwrap();
        click(1, &mut imgui, &mut app);
        let refused = state.borrow().completion().cloned().unwrap();
        assert_ne!(refused.0, retained.0);
        assert!(
            matches!(&refused.1, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if !report.applied)
        );
        assert_eq!(evidence.borrow().queued, [retained.0, refused.0]);
        assert_eq!(evidence.borrow().finished, [retained, refused]);
        assert_eq!(
            app.current_frame().unwrap().workspace(),
            before_stale.workspace()
        );
        let mut ready = app.request_exit().exit_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready {
            assert!(Instant::now() < deadline);
            ready = if let Some(frame) = app.current_frame() {
                let actions = frame.workspace_intent().decision_prompt().map(|prompt| {
                    EditorAction::WorkspaceIntent(
                        prompt.resolve(nara_tooling::EditorCloseDecision::Discard),
                    )
                });
                app.submit_and_drive(
                    EditorActionBatch::new(frame.revision(), actions),
                    Duration::ZERO,
                )
                .update()
                .exit_ready()
            } else {
                app.advance_project_open().exit_ready()
            };
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
