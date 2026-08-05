use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiFrameJournal, UiJournalError, UiLayer,
    UiRoot, UiRootKey, UiRouteStamp, UiStyle, UiSurfaceId, UiTheme,
};

fn snapshot(revision: u64) -> nara_ui_kernel::UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(revision)
        .with_root(UiRoot::new(
            UiRootKey::runtime("test", "root").unwrap(),
            UiLayer::Base,
            UiStyle::fill(),
        ))
        .build()
        .unwrap()
}

fn environment(surface: UiSurfaceId, revision: u64) -> UiEnvironment {
    UiEnvironment::new(surface, 100.0, 100.0, 1.0, revision, revision, 1, 1).unwrap()
}

#[test]
fn compile_without_submission_never_grants_route_authority() {
    let surface = UiSurfaceId::new(1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let _compiled = compiler
        .compile(&snapshot(1), environment(surface, 1), UiTheme::default())
        .unwrap();
    let journal = UiFrameJournal::default();
    assert_eq!(
        journal.resolve(UiRouteStamp::new(surface, 1)).unwrap_err(),
        UiJournalError::CompiledButNotPresented { surface }
    );
}

#[test]
fn journal_keeps_current_and_two_prior_exact_pairs() {
    let surface = UiSurfaceId::new(1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let mut journal = UiFrameJournal::default();
    let mut frames = Vec::new();
    for revision in 1..=4 {
        let frame = compiler
            .compile(
                &snapshot(revision),
                environment(surface, revision),
                UiTheme::default(),
            )
            .unwrap();
        journal.submit(Arc::clone(&frame), 100 + revision).unwrap();
        frames.push(frame);
    }

    for (epoch, expected) in [(104, 3usize), (103, 2), (102, 1)] {
        assert_eq!(
            journal
                .resolve(UiRouteStamp::new(surface, epoch))
                .unwrap()
                .revision(),
            frames[expected].revision()
        );
    }
    assert_eq!(
        journal
            .resolve(UiRouteStamp::new(surface, 101))
            .unwrap_err(),
        UiJournalError::FrameRetired {
            surface,
            presentation_epoch: 101,
        }
    );
}

#[test]
fn retired_surface_rejects_an_exact_previously_presented_route() {
    let surface = UiSurfaceId::new(2).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot(1), environment(surface, 1), UiTheme::default())
        .unwrap();
    let mut journal = UiFrameJournal::default();
    let receipt = journal.submit(frame, 9).unwrap();
    assert!(journal.retire_surface(surface));
    assert_eq!(
        journal
            .resolve(UiRouteStamp::new(surface, receipt.presentation_epoch()))
            .unwrap_err(),
        UiJournalError::SurfaceRetired {
            surface,
            presentation_epoch: 9,
        }
    );
}

#[test]
fn submitted_receipt_matches_the_exact_frame_and_environment() {
    let surface = UiSurfaceId::new(3).unwrap();
    let environment = environment(surface, 7);
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot(1), environment, UiTheme::default())
        .unwrap();
    let mut journal = UiFrameJournal::default();
    let receipt = journal.submit(Arc::clone(&frame), 44).unwrap();
    assert_eq!(receipt.surface_id(), surface);
    assert_eq!(receipt.frame_revision(), frame.revision());
    assert_eq!(receipt.route_revision(), environment.route_revision());
    assert_eq!(
        receipt.presentation_revision(),
        environment.presentation_revision()
    );
}
