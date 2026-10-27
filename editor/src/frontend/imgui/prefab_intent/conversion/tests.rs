use std::sync::Arc;

use dear_imgui_rs::{Condition, Context, MouseButton};
use nara::{
    diagnostic::{Diagnostic, DiagnosticCode, SafeSummary},
    scene::{PrefabInstancePathSegment, SceneAuthoringRevision, SceneEntityId},
    tooling::{
        EditorAction, EditorAuthoringReadiness, EditorFrameClock, EditorPersistenceView,
        EditorPlayState, EditorPlayView, EditorPrefabConversionMappingRow,
        EditorPrefabConversionRejection, EditorPrefabConversionReview, EditorProjectContentView,
        EditorProjectReadinessView, EditorProjectView, EditorRuntimeAvailability,
        EditorWorkspaceIntentView,
    },
};

use super::*;
use crate::{
    frontend::imgui::{
        PanelActionNotDispatched,
        submission::{self, PanelOrigin, Recipient},
        submission_tests::Harness,
        ui_interaction_tests::{clear_controls, control_rect},
    },
    workspace::EditorPanelInstanceId,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn operation_id(sequence: u64) -> EditorPrefabConversionId {
    let frame = EditorFrameClock::new().publish(
        nara::tooling::EditorWorkspace::new().authoring_model(),
        EditorProjectView::new(
            EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
            Default::default(),
            EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
            EditorPersistenceView::Idle,
            EditorProjectContentView::NotInstalled,
            EditorWorkspaceIntentView::new(None, None, None, None, None),
        ),
        Default::default(),
        nara::tooling::EditorReopenTarget::Unavailable,
        None,
    );
    EditorPrefabConversionId::new(frame.revision(), sequence)
}

fn basis() -> EditorDocumentQueryBasis {
    EditorDocumentQueryBasis {
        document: EditorDocumentId::from_raw(17),
        revision: SceneAuthoringRevision::initial(),
        schema: None,
    }
}

fn view(phase: EditorPrefabConversionPhase) -> EditorPrefabConversionView {
    let basis = basis();
    EditorPrefabConversionView {
        operation: Some(operation_id(1)),
        document: Some(basis.document),
        anchor: Some(Arc::from("original-anchor")),
        phase,
        review: (phase == EditorPrefabConversionPhase::Review).then(|| {
            Arc::new(EditorPrefabConversionReview {
                basis,
                offset: 0,
                total_entities: 3,
                rows: vec![EditorPrefabConversionMappingRow {
                    origin: Arc::from("outer / nested / leaf"),
                    entity: id("fresh-local-a"),
                }],
                next_offset: Some(1),
                source_count: 2,
                working_copy_count: 1,
            })
        }),
        result: None,
    }
}

fn pending(
    command: ConversionCommand,
    operation: Option<EditorPrefabConversionId>,
) -> ConversionSubmission {
    let mut next = 0;
    let mut submissions = Vec::new();
    let id = submission::enqueue(
        &mut next,
        &mut submissions,
        PanelOrigin {
            instance: EditorPanelInstanceId(1),
            incarnation: Default::default(),
        },
        0,
        Recipient::PrefabIntent,
    )
    .unwrap();
    ConversionSubmission {
        id,
        operation,
        command,
    }
}

struct PaintHarness {
    context: Context,
    state: ConversionUi,
}

impl PaintHarness {
    fn new() -> Self {
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([900.0, 1000.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        Self {
            context,
            state: ConversionUi::default(),
        }
    }

    fn draw(
        &mut self,
        view: &EditorPrefabConversionView,
        editing: bool,
    ) -> (bool, Option<EditorPrefabConversionAction>) {
        clear_controls();
        let ui = self.context.frame();
        let result = ui
            .window("Prefab conversion")
            .position([10.0, 10.0], Condition::Always)
            .size([850.0, 950.0], Condition::Always)
            .build(|| {
                self.state
                    .paint(ui, view, Some("Original.scene.json"), editing)
            })
            .unwrap();
        let _ = self.context.render_legacy();
        result
    }

    fn click(
        &mut self,
        view: &EditorPrefabConversionView,
        label: &str,
        editing: bool,
    ) -> Option<EditorPrefabConversionAction> {
        for _ in 0..3 {
            assert!(self.draw(view, editing).1.is_none());
        }
        let rect = control_rect(&format!("Prefab Intent: {label}")).unwrap();
        self.context.io_mut().add_mouse_pos_event([
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ]);
        assert!(self.draw(view, editing).1.is_none());
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        let pressed = self.draw(view, editing).1;
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let released = self.draw(view, editing).1;
        assert!(!(pressed.is_some() && released.is_some()));
        pressed.or(released)
    }
}

#[test]
fn scope_is_complete_selected_instance_and_requires_a_direct_linked_anchor() {
    assert_eq!(CONVERT_LABEL, "Convert Entire Instance to Local");
    assert!(CONVERSION_SCOPE.contains("all nested projected instances and overlay additions"));
    assert!(CONVERSION_SCOPE.contains("ordinary local children are preserved"));
    assert!(CONVERSION_SCOPE.contains("other instances stay linked"));
    let basis = basis();
    let path = PrefabInstancePath::new(id("direct-anchor"));
    assert_eq!(
        prepare_action(Some(&basis), Some(&path), true, true),
        Some(EditorPrefabConversionAction::Prepare {
            basis: basis.clone(),
            anchor: id("direct-anchor")
        }),
    );
    let nested = PrefabInstancePath {
        anchor: path.anchor.clone(),
        steps: vec![PrefabInstancePathSegment::Addition { index: 0 }],
    };
    for (path, linked, enabled) in [
        (&nested, true, true),
        (&path, false, true),
        (&path, true, false),
    ] {
        assert!(prepare_action(Some(&basis), Some(path), linked, enabled).is_none());
    }
}

#[test]
fn real_prepare_control_only_queues_the_chosen_direct_anchor_on_click() {
    let mut ui = PaintHarness::new();
    let basis = basis();
    let mut path = PrefabInstancePath::new(id("chosen-anchor"));
    let paint = |context: &mut Context, path: &PrefabInstancePath| {
        clear_controls();
        let frame = context.frame();
        let result = frame
            .window("Prepare complete conversion")
            .position([10.0, 10.0], Condition::Always)
            .size([850.0, 500.0], Condition::Always)
            .build(|| paint_prepare(frame, Some(&basis), Some(path), true, true))
            .unwrap();
        let _ = context.render_legacy();
        result
    };
    for nested in [false, true] {
        path.steps = if nested {
            vec![PrefabInstancePathSegment::Addition { index: 0 }]
        } else {
            Vec::new()
        };
        for _ in 0..3 {
            assert!(paint(&mut ui.context, &path).is_none());
        }
        assert!(control_rect("Prefab Intent conversion scope").is_some());
        let rect = control_rect(&format!("Prefab Intent: {CONVERT_LABEL}")).unwrap();
        ui.context.io_mut().add_mouse_pos_event([
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ]);
        assert!(paint(&mut ui.context, &path).is_none());
        ui.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        let pressed = paint(&mut ui.context, &path);
        ui.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let released = paint(&mut ui.context, &path);
        assert!(!(pressed.is_some() && released.is_some()));
        assert_eq!(
            pressed.or(released),
            (!nested).then(|| EditorPrefabConversionAction::Prepare {
                basis: basis.clone(),
                anchor: id("chosen-anchor"),
            }),
        );
    }
}

#[test]
fn repaint_only_observes_host_work_and_never_prepares_or_commits() {
    let mut ui = PaintHarness::new();
    let view = view(EditorPrefabConversionPhase::Preparing);
    for _ in 0..4 {
        assert_eq!(ui.draw(&view, true), (true, None));
        assert_eq!(ui.state.operation, view.operation);
        assert!(control_rect("Prefab Intent conversion phase: Preparing").is_some());
        assert!(control_rect("Prefab Intent conversion result: Committed").is_none());
    }
    assert_eq!(ui.state.owner.as_ref().unwrap().anchor, "original-anchor");
}

#[test]
fn accepted_commit_waits_for_exact_terminal_result_and_rejects_other_operation_results() {
    let mut ui = PaintHarness::new();
    let mut view = view(EditorPrefabConversionPhase::Review);
    ui.draw(&view, true);
    let operation = view.operation.unwrap();
    let queued = pending(ConversionCommand::Commit, Some(operation));
    ui.state.pending = Some(queued);
    assert!(ui.state.action_finished(
        queued.id,
        &PanelActionDispatch::Dispatched(EditorActionOutcome::PrefabConversion(
            EditorPrefabConversionActionOutcome::Accepted {
                operation,
                phase: EditorPrefabConversionPhase::Validating,
            },
        )),
    ));
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion phase: Validating").is_some());
    assert!(control_rect("Prefab Intent conversion result: Committed").is_none());
    assert_eq!(
        ui.click(&view, "Cancel conversion", true),
        Some(EditorPrefabConversionAction::Cancel { operation }),
    );
    view.phase = EditorPrefabConversionPhase::Terminal;
    view.result = Some(EditorPrefabConversionResult::Cancelled {
        operation: operation_id(2),
    });
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion result: Cancelled").is_none());
    view.result = Some(EditorPrefabConversionResult::Cancelled { operation });
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion result: Cancelled").is_some());
    assert!(control_rect("Prefab Intent conversion result: Committed").is_none());
    assert_eq!(
        ui.click(&view, "Acknowledge conversion result", false),
        Some(EditorPrefabConversionAction::Acknowledge { operation }),
    );
}

#[test]
fn review_pages_and_commit_retain_host_mapping_and_operation_identity() {
    let mut ui = PaintHarness::new();
    let mut view = view(EditorPrefabConversionPhase::Review);
    let operation = view.operation.unwrap();
    assert_eq!(
        ui.click(&view, "Next conversion mapping page", true),
        Some(EditorPrefabConversionAction::ReviewPage {
            operation,
            offset: 1
        }),
    );
    let review = Arc::make_mut(view.review.as_mut().unwrap());
    review.offset = 1;
    review.rows = vec![EditorPrefabConversionMappingRow {
        origin: Arc::from("outer / addition / nested / leaf"),
        entity: id("fresh-local-b"),
    }];
    review.next_offset = None;
    ui.draw(&view, true);
    assert!(
        control_rect(
            "Prefab Intent conversion mapping: outer / addition / nested / leaf -> fresh-local-b",
        )
        .is_some()
    );
    assert_eq!(
        ui.click(&view, "First conversion mapping page", true),
        Some(EditorPrefabConversionAction::ReviewPage {
            operation,
            offset: 0
        }),
    );
    assert!(
        ui.click(&view, "Next conversion mapping page", true)
            .is_none()
    );
    assert_eq!(
        ui.click(&view, "Commit reviewed conversion", true),
        Some(EditorPrefabConversionAction::Commit { operation }),
    );
    assert!(
        ui.click(&view, "Commit reviewed conversion", false)
            .is_none()
    );
}

#[test]
fn cancel_pending_work_uses_original_operation_even_when_editing_is_disabled() {
    for phase in [
        EditorPrefabConversionPhase::Preparing,
        EditorPrefabConversionPhase::Review,
        EditorPrefabConversionPhase::Validating,
    ] {
        let mut ui = PaintHarness::new();
        let view = view(phase);
        assert_eq!(
            ui.click(&view, "Cancel conversion", false),
            Some(EditorPrefabConversionAction::Cancel {
                operation: view.operation.unwrap()
            }),
        );
    }
}

#[test]
fn failed_terminal_renders_diagnostics_and_acknowledges_same_operation() {
    let mut ui = PaintHarness::new();
    let mut view = view(EditorPrefabConversionPhase::Terminal);
    let operation = view.operation.unwrap();
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(
        DiagnosticCode::new("test.conversion-source-changed").unwrap(),
        SafeSummary::new("The selected Prefab source changed.").unwrap(),
    ));
    view.result = Some(EditorPrefabConversionResult::Failed {
        operation,
        reason: EditorPrefabConversionRejection::StaleSource,
        diagnostics,
    });
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion result: Failed").is_some());
    assert!(
        control_rect("Prefab Intent conversion diagnostic: test.conversion-source-changed")
            .is_some()
    );
    assert_eq!(
        ui.click(&view, "Acknowledge conversion result", false),
        Some(EditorPrefabConversionAction::Acknowledge { operation }),
    );
}

#[test]
fn committed_terminal_displays_the_actual_public_source_receipt() {
    use nara::{
        asset::AssetRef,
        reflect::AuthoringSchemaBundle,
        scene::{
            AuthoringDocumentSource, AuthoringSchemaBasis, PrefabDocument, PrefabDocumentCandidate,
            PrefabInstance, ResolvedPrefabSource, SceneDocument, SceneDocumentCandidate,
            SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
        },
        tooling::{EditorDocumentSourceBinding, EditorWorkspace},
    };

    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let source_ref = AssetRef::path("prefabs/linked.prefab.json").unwrap();
    let dependency = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]);
    let published = PrefabDocumentCandidate::decode_json_str(&dependency.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(AuthoringDocumentSource::new(source_ref.clone()), &basis)
        .unwrap()
        .publish();
    let source = ResolvedPrefabSource::bound_authoring(
        Arc::new(published),
        SourceDocumentKey::named(
            "dc00fda3-201b-4c34-9131-dc8347d88666".parse().unwrap(),
            "ui-conversion-receipt",
            SourceDocumentKind::Prefab,
        )
        .unwrap(),
    );
    let mut anchor = SceneEntityRecord::new(id("original-anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: source_ref,
        overrides: None,
    });
    let document = SceneDocument::from_ordered_roots([anchor]);
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/original.scene.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let mut workspace = EditorWorkspace::new();
    let document = workspace
        .open_authoring_scene_document(
            "Original.scene.json",
            EditorDocumentSourceBinding::new("scenes/original.scene.json"),
            published,
        )
        .unwrap()
        .opened_document
        .unwrap();
    let prepared = workspace
        .request_complete_prefab_conversion(
            &workspace.document_query_basis(document).unwrap(),
            &id("original-anchor"),
            Default::default(),
        )
        .unwrap()
        .prepare_with_loader(None, |_, _| {
            Ok::<_, std::convert::Infallible>(Some(source.clone()))
        })
        .unwrap();
    let receipt = workspace
        .commit_complete_prefab_conversion_with_loader(prepared, |_| {
            Ok::<_, std::convert::Infallible>(Some(source.clone()))
        })
        .unwrap();
    assert!(receipt.report().applied);
    assert_eq!(receipt.entities().len(), 1);

    let mut ui = PaintHarness::new();
    let mut view = view(EditorPrefabConversionPhase::Terminal);
    let operation = view.operation.unwrap();
    view.document = Some(document);
    view.result = Some(EditorPrefabConversionResult::Committed {
        operation,
        receipt: Arc::new(receipt),
    });
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion result: Committed").is_some());
    assert_eq!(
        ui.click(&view, "Acknowledge conversion result", false),
        Some(EditorPrefabConversionAction::Acknowledge { operation }),
    );
}

#[test]
fn mapping_focus_uses_real_input_without_repreparing_or_changing_the_review() {
    let mut ui = PaintHarness::new();
    let view = view(EditorPrefabConversionPhase::Review);
    ui.draw(&view, true);
    assert!(ui.click(&view, "Local fresh-local-a", true).is_none());
    assert_eq!(ui.state.mapping_focus, Some(id("fresh-local-a")));
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion source detail").is_some());
    assert_eq!(ui.state.operation, view.operation);
    assert_eq!(ui.state.phase, EditorPrefabConversionPhase::Review);
    assert!(ui.state.pending.is_none());
}

#[test]
fn host_operation_is_recovered_by_a_fresh_panel_and_keeps_its_original_source_label() {
    let view = view(EditorPrefabConversionPhase::Review);
    let mut ui = PaintHarness::new();
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion owner: 17 / original-anchor").is_some());
    ui.state
        .observe(&view, Some("An unrelated active document"));
    assert_eq!(
        ui.state.owner.as_ref().unwrap().title,
        "Original.scene.json"
    );
    assert_eq!(ui.state.operation, view.operation);
    ui.state = ConversionUi::default();
    ui.draw(&view, true);
    assert_eq!(ui.state.operation, view.operation);
    assert_eq!(ui.state.owner.as_ref().unwrap().document, basis().document);
    assert_eq!(
        ui.click(&view, "Cancel conversion", true),
        Some(EditorPrefabConversionAction::Cancel {
            operation: view.operation.unwrap()
        }),
    );
}

#[test]
fn acknowledgement_by_another_panel_releases_only_the_display_state() {
    let mut ui = PaintHarness::new();
    let mut view = view(EditorPrefabConversionPhase::Terminal);
    let operation = view.operation.unwrap();
    view.result = Some(EditorPrefabConversionResult::Cancelled { operation });
    assert_eq!(ui.draw(&view, true), (true, None));
    assert_eq!(ui.state.operation, Some(operation));
    assert_eq!(
        ui.draw(&EditorPrefabConversionView::default(), true),
        (false, None)
    );
    assert!(ui.state.operation.is_none());
    assert!(ui.state.owner.is_none());
}

#[test]
fn dispatch_refusal_preserves_the_same_review_and_never_claims_completion() {
    let mut ui = PaintHarness::new();
    let view = view(EditorPrefabConversionPhase::Review);
    ui.draw(&view, true);
    let operation = view.operation.unwrap();
    let queued = pending(ConversionCommand::Commit, Some(operation));
    ui.state.pending = Some(queued);
    assert!(ui.state.action_finished(
        queued.id,
        &PanelActionDispatch::Dispatched(EditorActionOutcome::PrefabConversion(
            EditorPrefabConversionActionOutcome::Rejected {
                operation: Some(operation),
                reason: EditorPrefabConversionRejection::WrongPhase,
            },
        )),
    ));
    assert!(ui.state.notice.as_deref().unwrap().contains("WrongPhase"));
    ui.draw(&view, true);
    assert!(control_rect("Prefab Intent conversion phase: Review").is_some());
    assert!(control_rect("Prefab Intent conversion result: Committed").is_none());
    assert_eq!(
        ui.click(&view, "Cancel conversion", true),
        Some(EditorPrefabConversionAction::Cancel { operation }),
    );
}

#[test]
fn conversion_commands_use_prefab_recipient_and_settle_separately_from_revert() {
    let mut harness = Harness::new(1);
    let operation =
        EditorPrefabConversionId::new(harness.application.current_frame().unwrap().revision(), 1);
    let mut once = false;
    harness.panels[0].1.borrow_mut().paint = Some(Box::new(move |context| {
        if once {
            return;
        }
        once = true;
        let mut state = std::mem::take(&mut context.widgets.prefab_intent);
        state.conversion.submit(
            context,
            EditorPrefabConversionAction::ReviewPage {
                operation,
                offset: 64,
            },
        );
        context.widgets.prefab_intent = state;
    }));
    let candidate = harness.draw();
    assert!(
        candidate
            .ledger
            .submissions
            .iter()
            .any(|submission| matches!(submission.recipient, Recipient::PrefabIntent,))
    );
    let actions = candidate.batch.into_actions();
    assert!(matches!(actions.as_slice(), [
        EditorAction::PrefabConversion(EditorPrefabConversionAction::ReviewPage {
            operation: actual, offset: 64,
        }),
    ] if *actual == operation));
    let panel = harness.panels[0].0;
    let state = &mut harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .prefab_intent;
    let pending = state.conversion.pending.unwrap().id;
    let accepted = PanelActionDispatch::Dispatched(EditorActionOutcome::PrefabConversion(
        EditorPrefabConversionActionOutcome::Accepted {
            operation,
            phase: EditorPrefabConversionPhase::Review,
        },
    ));
    state.action_finished(pending, accepted);
    assert_eq!(state.conversion.operation, Some(operation));
    assert!(state.pending.is_none());
    assert!(
        state.result.is_none(),
        "conversion acceptance must not enter the Revert result"
    );
    state.action_finished(
        pending,
        PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded),
    );
    assert_eq!(state.conversion.operation, Some(operation));
    assert!(
        state.conversion.notice.is_none(),
        "duplicate settlement must be ignored"
    );
    assert!(harness.panels[0].1.borrow().finished.is_empty());
}
