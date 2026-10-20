use std::{path::PathBuf, sync::Arc};

use dear_imgui_rs::{Condition, Context, MouseButton};
use nara::tooling::{
    EditorAction, EditorAuthoringReadiness, EditorDocumentId, EditorDocumentOpenId,
    EditorDocumentOpenPhase, EditorDocumentOpenResult, EditorDocumentOpenView, EditorFrameClock,
    EditorFrameModel, EditorPersistenceOperation, EditorPersistenceView, EditorPlayState,
    EditorPlayView, EditorProjectBrowserAction, EditorProjectContentView,
    EditorProjectReadinessView, EditorProjectView, EditorReopenTarget, EditorRuntimeAvailability,
    EditorWorkspace, EditorWorkspaceIntentView,
};

use super::show_document_open;

#[test]
fn preparing_cancel_uses_the_exact_request_and_later_phases_do_not_offer_it() {
    let base = empty_frame();
    let request = EditorDocumentOpenId::new(base.revision(), 17);
    let pending = |phase| {
        base.clone().with_document_open(EditorDocumentOpenView::new(
            Some((request, Arc::from("scenes/external.scene.json"), phase)),
            None,
        ))
    };
    let preparing = pending(EditorDocumentOpenPhase::Preparing);
    let mut context = context();
    let cancel = settle(&mut context, &preparing);
    assert_eq!(
        click(&mut context, &preparing, cancel),
        vec![EditorAction::ProjectBrowser(
            EditorProjectBrowserAction::CancelOpen { request }
        )]
    );

    for phase in [
        EditorDocumentOpenPhase::Cancelling,
        EditorDocumentOpenPhase::AwaitingClose {
            document: EditorDocumentId::from_raw(4),
        },
    ] {
        let waiting = pending(phase);
        settle(&mut context, &waiting);
        assert!(
            click(&mut context, &waiting, cancel).is_empty(),
            "{phase:?} must leave cancellation to physical retirement or the close prompt"
        );
    }
}

#[test]
fn every_terminal_result_dismisses_only_its_exact_request() {
    for outcome in 0..3 {
        let base = empty_frame();
        let request = EditorDocumentOpenId::new(base.revision(), 23);
        let result = match outcome {
            0 => EditorDocumentOpenResult::Opened {
                request,
                document: EditorDocumentId::from_raw(8),
            },
            1 => EditorDocumentOpenResult::Rejected {
                request,
                code: "project.editor.document-source-changed",
            },
            _ => EditorDocumentOpenResult::Cancelled { request },
        };
        let frame = base.with_document_open(EditorDocumentOpenView::new(None, Some(result)));
        let mut context = context();
        let dismiss = settle(&mut context, &frame);
        assert_eq!(
            click(&mut context, &frame, dismiss),
            vec![EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::AcknowledgeOpen { request }
            )],
            "{result:?}"
        );
    }
}

fn empty_frame() -> EditorFrameModel {
    EditorFrameClock::new().publish(
        EditorWorkspace::new().authoring_model(),
        EditorProjectView::new(
            EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
            Default::default(),
            EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
            EditorPersistenceView::new(EditorPersistenceOperation::Idle, None),
            EditorProjectContentView::NotInstalled,
            EditorWorkspaceIntentView::new(None, None, None, None, None),
        ),
        Default::default(),
        EditorReopenTarget::Unavailable,
        None,
    )
}

fn context() -> Context {
    let mut context = Context::create();
    context.set_ini_filename::<PathBuf>(None).unwrap();
    context.io_mut().set_display_size([800.0, 600.0]);
    context.io_mut().set_delta_time(1.0 / 60.0);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context
}

fn paint(context: &mut Context, frame: &EditorFrameModel) -> (Vec<EditorAction>, [f32; 2]) {
    let ui = context.frame();
    let mut actions = Vec::new();
    let mut center = None;
    ui.window("Browser document operation")
        .position([20.0, 20.0], Condition::Always)
        .size([600.0, 300.0], Condition::Always)
        .build(|| {
            show_document_open(ui, frame, &mut actions);
            let (min, max) = ui.item_rect();
            assert!(max[0] > min[0] && max[1] > min[1]);
            center = Some([(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5]);
        });
    let _ = context.render_legacy();
    (
        actions,
        center.expect("the document operation must render its controls or status"),
    )
}

fn settle(context: &mut Context, frame: &EditorFrameModel) -> [f32; 2] {
    let mut center = [0.0; 2];
    for _ in 0..3 {
        let painted = paint(context, frame);
        assert!(painted.0.is_empty());
        center = painted.1;
    }
    center
}

fn click(context: &mut Context, frame: &EditorFrameModel, point: [f32; 2]) -> Vec<EditorAction> {
    context.io_mut().add_mouse_pos_event(point);
    assert!(paint(context, frame).0.is_empty());
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    assert!(paint(context, frame).0.is_empty());
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    paint(context, frame).0
}
