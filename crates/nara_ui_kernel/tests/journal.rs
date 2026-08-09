use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiFrameJournal, UiJournalError, UiLayer,
    UiRoot, UiRootKey, UiRouteStamp, UiSize, UiStyle, UiSurfaceContentTransform,
    UiSurfaceGenerations, UiSurfaceId, UiTheme,
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

fn surface_environment(surface: UiSurfaceId, slot: u64, route: u64) -> UiEnvironment {
    UiEnvironment::for_surface(
        surface,
        UiSurfaceContentTransform::direct(UiSize::new(100.0, 100.0).unwrap(), 1.0).unwrap(),
        UiSurfaceGenerations::new(slot, route, route, route).unwrap(),
        1,
        1,
    )
    .unwrap()
}

fn hosted_journal() -> (
    UiFrameJournal,
    nara_ui_kernel::__private::UiFrameJournalAuthority,
) {
    nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap()
}

#[test]
fn compile_without_submission_never_grants_route_authority() {
    let surface = UiSurfaceId::new(1).unwrap();
    let environment = environment(surface, 1);
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let _compiled = compiler
        .compile(&snapshot(1), environment, UiTheme::default())
        .unwrap();
    let journal = UiFrameJournal::default();
    assert_eq!(
        journal
            .resolve(UiRouteStamp::new(
                surface,
                environment.slot_generation(),
                environment.route_revision(),
                environment.geometry_generation(),
                1,
            ))
            .unwrap_err(),
        UiJournalError::CompiledButNotPresented { surface }
    );
}

#[test]
fn hosted_submission_authority_is_bound_to_one_journal_lineage() {
    let surface = UiSurfaceId::new(11).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot(1), environment(surface, 1), UiTheme::default())
        .unwrap();
    let (mut journal, authority) = hosted_journal();
    let (_foreign_journal, foreign_authority) = hosted_journal();
    journal.install_surface(surface, 1).unwrap();

    assert_eq!(
        foreign_authority
            .submit(&mut journal, Arc::clone(&frame), 10)
            .unwrap_err(),
        UiJournalError::SubmissionAuthorityMismatch
    );
    assert_eq!(
        journal.latest(surface).unwrap_err(),
        UiJournalError::CompiledButNotPresented { surface }
    );

    let receipt = authority.submit(&mut journal, frame, 10).unwrap();
    assert_eq!(receipt.surface_id(), surface);
}

#[test]
fn journal_keeps_current_and_two_prior_exact_pairs() {
    let surface = UiSurfaceId::new(1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let (mut journal, authority) = hosted_journal();
    journal.install_surface(surface, 1).unwrap();
    let mut frames = Vec::new();
    let mut routes = Vec::new();
    for revision in 1..=4 {
        let frame = compiler
            .compile(
                &snapshot(revision),
                environment(surface, revision),
                UiTheme::default(),
            )
            .unwrap();
        let receipt = authority
            .submit(&mut journal, Arc::clone(&frame), 100 + revision)
            .unwrap();
        routes.push(UiRouteStamp::from_receipt(receipt));
        frames.push(frame);
    }

    for (route_index, expected) in [(3usize, 3usize), (2, 2), (1, 1)] {
        assert_eq!(
            journal.resolve(routes[route_index]).unwrap().revision(),
            frames[expected].revision()
        );
    }
    assert_eq!(
        journal.resolve(routes[0]).unwrap_err(),
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
    let (mut journal, authority) = hosted_journal();
    journal.install_surface(surface, 1).unwrap();
    let receipt = authority.submit(&mut journal, frame, 9).unwrap();
    let retirement = journal.retire_surface(surface, 2).unwrap().unwrap();
    assert_eq!(retirement.surface_id(), surface);
    assert_eq!(retirement.slot_generation(), receipt.slot_generation());
    assert_eq!(retirement.retirement_generation().get(), 2);
    assert!(journal.retire_surface(surface, 3).unwrap().is_none());
    assert_eq!(
        journal
            .resolve(UiRouteStamp::from_receipt(receipt))
            .unwrap_err(),
        UiJournalError::SurfaceRetired {
            surface,
            presentation_epoch: 9,
        }
    );

    let late = compiler
        .compile(&snapshot(2), environment(surface, 2), UiTheme::default())
        .unwrap();
    assert_eq!(
        authority.submit(&mut journal, late, 10).unwrap_err(),
        UiJournalError::SurfaceNotInstalled { surface }
    );
    assert_eq!(
        journal.install_surface(surface, 1).unwrap_err(),
        UiJournalError::SurfaceIdentityRetired { surface }
    );
}

#[test]
fn installed_surface_can_retire_before_its_first_presentation() {
    let surface = UiSurfaceId::new(12).unwrap();
    let mut journal = UiFrameJournal::default();
    journal.install_surface(surface, 7).unwrap();
    assert_eq!(
        journal.install_surface(surface, 7).unwrap_err(),
        UiJournalError::SurfaceAlreadyInstalled { surface }
    );

    let retirement = journal.retire_surface(surface, 8).unwrap().unwrap();
    assert_eq!(retirement.surface_id(), surface);
    assert_eq!(retirement.slot_generation().get(), 7);
    assert_eq!(retirement.retirement_generation().get(), 8);
    assert!(journal.retire_surface(surface, 9).unwrap().is_none());
}

#[test]
fn newer_slot_generation_atomically_retires_old_routes() {
    let surface = UiSurfaceId::new(13).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let (mut journal, authority) = hosted_journal();
    journal.install_surface(surface, 1).unwrap();

    let first = compiler
        .compile(
            &snapshot(1),
            surface_environment(surface, 1, 1),
            UiTheme::default(),
        )
        .unwrap();
    let first_receipt = authority.submit(&mut journal, first, 10).unwrap();
    let successor = compiler
        .compile(
            &snapshot(2),
            surface_environment(surface, 2, 2),
            UiTheme::default(),
        )
        .unwrap();
    assert!(matches!(
        authority.submit(&mut journal, Arc::clone(&successor), 11),
        Err(UiJournalError::SlotGenerationMismatch { .. })
    ));
    journal.advance_surface_slot(surface, 1, 2).unwrap();
    let successor_receipt = authority.submit(&mut journal, successor, 11).unwrap();

    assert!(
        journal
            .resolve(UiRouteStamp::from_receipt(successor_receipt))
            .is_ok()
    );
    assert_eq!(
        journal
            .resolve(UiRouteStamp::from_receipt(first_receipt))
            .unwrap_err(),
        UiJournalError::SurfaceRetired {
            surface,
            presentation_epoch: 10,
        }
    );

    let rewound = compiler
        .compile(
            &snapshot(3),
            surface_environment(surface, 1, 3),
            UiTheme::default(),
        )
        .unwrap();
    assert!(matches!(
        authority.submit(&mut journal, rewound, 12),
        Err(UiJournalError::SlotGenerationMismatch { .. })
    ));

    assert!(matches!(
        journal.advance_surface_slot(surface, 1, 3),
        Err(UiJournalError::SlotGenerationMismatch { .. })
    ));
    assert!(matches!(
        journal.advance_surface_slot(surface, 2, 2),
        Err(UiJournalError::SlotGenerationDidNotAdvance { .. })
    ));
}

#[test]
fn submitted_receipt_matches_the_exact_frame_and_environment() {
    let surface = UiSurfaceId::new(3).unwrap();
    let environment = environment(surface, 7);
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot(1), environment, UiTheme::default())
        .unwrap();
    let (mut journal, authority) = hosted_journal();
    journal
        .install_surface(surface, environment.slot_generation().get())
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 44)
        .unwrap();
    assert_eq!(receipt.surface_id(), surface);
    assert_eq!(receipt.frame_revision(), frame.revision());
    assert_eq!(receipt.slot_generation(), environment.slot_generation());
    assert_eq!(receipt.route_revision(), environment.route_revision());
    assert_eq!(
        receipt.geometry_generation(),
        environment.geometry_generation()
    );
    assert_eq!(
        receipt.presentation_revision(),
        environment.presentation_revision()
    );
}

#[test]
fn exact_route_generations_cannot_be_replayed_at_the_same_epoch() {
    let surface = UiSurfaceId::new(4).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot(1), environment(surface, 1), UiTheme::default())
        .unwrap();
    let (mut journal, authority) = hosted_journal();
    journal.install_surface(surface, 1).unwrap();
    let receipt = authority.submit(&mut journal, frame, 9).unwrap();
    let wrong_geometry = UiSurfaceGenerations::new(
        receipt.slot_generation().get(),
        receipt.route_revision().get(),
        receipt.geometry_generation().get() + 1,
        receipt.presentation_revision().get(),
    )
    .unwrap();

    assert_eq!(
        journal
            .resolve(UiRouteStamp::new(
                surface,
                wrong_geometry.slot_generation(),
                wrong_geometry.route_revision(),
                wrong_geometry.geometry_generation(),
                9,
            ))
            .unwrap_err(),
        UiJournalError::RouteGenerationMismatch {
            surface,
            presentation_epoch: 9,
        }
    );
}
