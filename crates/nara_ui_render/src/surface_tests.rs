use std::sync::Arc;

use nara_render::RenderTarget;
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiFrame, UiFrameJournal,
    UiImageId, UiLayer, UiLimits, UiLocalKey, UiNode, UiPresentationReceipt, UiRoot, UiRootKey,
    UiRouteStamp, UiSize, UiStyle, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId,
    UiTheme, UiVisual,
};
use nara_window::WindowId;

use crate::{
    UiPreparedImages, UiPresentationBackendGeneration, UiPresentationLimits, UiPresentationOutcome,
    UiSurfacePresentationEvent, UiSurfacePresenter,
    advanced::{
        UiBackendPresentationFrame, UiBackendSubmission, UiSubmittedBackendSubmission,
        bind_backend_submission_batch,
    },
};

const SURFACE: u64 = 41;
const SLOT: u64 = 7;
const WHITE: UiColor = UiColor::rgba(1.0, 1.0, 1.0, 1.0);

fn fixture(surface: u64, slot: u64) -> (UiCompiler, UiSurfacePresenter) {
    let font = bundled_font_face().unwrap();
    let compiler = UiCompiler::new(font.clone(), UiLimits::default()).unwrap();
    let presenter = UiSurfacePresenter::with_fonts(
        UiSurfaceId::new(surface).unwrap(),
        slot,
        RenderTarget::Window(WindowId::PRIMARY),
        vec![font],
        UiLimits::default(),
        UiPresentationLimits::default(),
    )
    .unwrap();
    (compiler, presenter)
}

fn frame(
    compiler: &mut UiCompiler,
    surface: u64,
    slot: u64,
    revision: u64,
    visual: UiVisual,
) -> Arc<UiFrame> {
    let snapshot = UiAuthoringSnapshotBuilder::new(revision)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("surface-presenter", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(
                UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0))
                    .with_key(UiLocalKey::runtime("surface-presenter", "node").unwrap())
                    .with_visual(visual),
            ),
        )
        .build()
        .unwrap();
    let transform =
        UiSurfaceContentTransform::direct(UiSize::new(64.0, 64.0).unwrap(), 1.0).unwrap();
    let environment = UiEnvironment::for_surface(
        UiSurfaceId::new(surface).unwrap(),
        transform,
        UiSurfaceGenerations::new(slot, revision, revision, revision).unwrap(),
        1,
        1,
    )
    .unwrap();
    compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap()
}

fn original_submission(backend: UiBackendPresentationFrame) -> UiBackendSubmission {
    let (packets, mut submissions) = backend.into_parts();
    assert_eq!(packets.len(), 1);
    assert_eq!(submissions.len(), 1);
    let submission = submissions.pop().unwrap();
    assert!(Arc::ptr_eq(&packets[0], submission.submission().packet()));
    submission
}

// Explicit CPU backend control: binding and completion exercise original permits, not native GPU
// presentation or native-use completion evidence.
fn submitted(backend: UiBackendPresentationFrame) -> UiSubmittedBackendSubmission {
    let generation = UiPresentationBackendGeneration::new(17, 3).unwrap();
    let mut submissions =
        bind_backend_submission_batch(vec![original_submission(backend)], generation).unwrap();
    assert_eq!(submissions.len(), 1);
    submissions.pop().unwrap()
}

fn presented_receipt(events: &[UiSurfacePresentationEvent]) -> UiPresentationReceipt {
    let [UiSurfacePresentationEvent::Presented(receipt)] = events else {
        panic!("expected one exact presentation, got {events:?}");
    };
    *receipt
}

fn present(presenter: &mut UiSurfacePresenter, frame: Arc<UiFrame>) -> UiPresentationReceipt {
    let backend = presenter
        .prepare(Arc::clone(&frame), &UiPreparedImages::new())
        .unwrap();
    let submitted = submitted(backend);
    let identity = submitted.submission().identity();
    assert!(Arc::ptr_eq(
        &submitted.submission().packet().frame(),
        &frame
    ));
    presenter.return_outcome(submitted.presented()).unwrap();
    let receipt = presented_receipt(&presenter.drain().unwrap());
    assert!(identity.matches_receipt(receipt));
    let target = presenter
        .journal()
        .resolve(UiRouteStamp::from_receipt(receipt))
        .unwrap();
    assert!(Arc::ptr_eq(target.frame(), &frame));
    assert_eq!(target.receipt(), receipt);
    receipt
}

#[test]
fn prepare_has_no_route_and_dropping_the_original_submission_aborts_and_settles() {
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let candidate = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
    let surface = candidate.environment().surface_id();
    assert!(presenter.is_quiescent());
    let backend = presenter
        .prepare(Arc::clone(&candidate), &UiPreparedImages::new())
        .unwrap();
    let submission = original_submission(backend);
    let identity = submission.submission().identity();
    assert!(Arc::ptr_eq(
        &submission.submission().packet().frame(),
        &candidate,
    ));
    assert!(presenter.journal().latest(surface).is_err());
    assert!(!presenter.is_quiescent());
    assert!(
        presenter
            .prepare(Arc::clone(&candidate), &UiPreparedImages::new())
            .is_err()
    );
    assert!(presenter.retry().is_err());
    assert!(presenter.drain().is_err());

    drop(submission);
    assert!(!presenter.is_quiescent());
    let events = presenter.drain().unwrap();
    let [UiSurfacePresentationEvent::Terminal(UiPresentationOutcome::Aborted { attempt, .. })] =
        events.as_slice()
    else {
        panic!("dropping the original permit must abort exactly once: {events:?}");
    };
    assert_eq!(attempt.backend_submission(), Some(identity));
    assert!(presenter.journal().latest(surface).is_err());
    assert!(presenter.is_quiescent());
    assert!(presenter.drain().unwrap().is_empty());
    present(&mut presenter, candidate);
}

#[test]
fn foreign_completion_is_returned_intact_to_its_original_owner() {
    let (mut compiler, mut owner) = fixture(SURFACE, SLOT);
    let (_, mut foreign) = fixture(SURFACE, SLOT);
    let candidate = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
    let surface = candidate.environment().surface_id();
    let original = submitted(
        owner
            .prepare(Arc::clone(&candidate), &UiPreparedImages::new())
            .unwrap(),
    );
    let other = submitted(
        foreign
            .prepare(Arc::clone(&candidate), &UiPreparedImages::new())
            .unwrap(),
    );
    let identity = original.submission().identity();
    assert_eq!(other.submission().identity(), identity);

    let completion = foreign
        .return_outcome(original.presented())
        .expect_err("equal identities must not let a foreign owner consume the original permit");
    assert!(foreign.drain().is_err());
    assert!(foreign.journal().latest(surface).is_err());
    assert!(owner.journal().latest(surface).is_err());
    owner.return_outcome(completion).unwrap();
    assert!(owner.journal().latest(surface).is_err());
    let receipt = presented_receipt(&owner.drain().unwrap());
    assert!(identity.matches_receipt(receipt));
    assert!(Arc::ptr_eq(
        owner.journal().latest(surface).unwrap().frame(),
        &candidate,
    ));
    assert!(owner.is_quiescent());

    foreign.return_outcome(other.presented()).unwrap();
    let other_receipt = presented_receipt(&foreign.drain().unwrap());
    assert!(identity.matches_receipt(other_receipt));
    assert!(foreign.is_quiescent());
    assert!(owner.drain().unwrap().is_empty());
}

#[test]
fn presented_frames_keep_three_exact_routes_and_retirement_isolates_a_new_surface() {
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let surface = UiSurfaceId::new(SURFACE).unwrap();
    let mut history = Vec::new();
    for revision in 1..=4 {
        let candidate = frame(
            &mut compiler,
            SURFACE,
            SLOT,
            revision,
            UiVisual::solid(WHITE),
        );
        let receipt = present(&mut presenter, Arc::clone(&candidate));
        assert_eq!(receipt.frame_revision(), candidate.revision());
        assert_eq!(receipt.generations(), candidate.environment().generations());
        history.push((candidate, receipt));
    }
    assert!(
        history
            .windows(2)
            .all(|pair| pair[0].1.presentation_epoch() < pair[1].1.presentation_epoch())
    );

    // This access is deliberately through a shared presenter and a shared journal reference.
    let observer: &UiSurfacePresenter = &presenter;
    let journal: &UiFrameJournal = observer.journal();
    assert!(
        journal
            .resolve(UiRouteStamp::from_receipt(history[0].1))
            .is_err()
    );
    for (candidate, receipt) in &history[1..] {
        let target = journal
            .resolve(UiRouteStamp::from_receipt(*receipt))
            .unwrap();
        assert_eq!(target.receipt(), *receipt);
        assert!(Arc::ptr_eq(target.frame(), candidate));
    }
    let mut detached = journal.clone();
    detached.retire_surface(surface, 8).unwrap().unwrap();
    assert_eq!(
        presenter.journal().latest(surface).unwrap().receipt(),
        history[3].1,
    );

    let token = presenter.retire(9).unwrap().unwrap();
    assert_eq!(token.surface_id(), surface);
    assert_eq!(token.slot_generation().get(), SLOT);
    assert_eq!(token.retirement_generation().get(), 9);
    for (_, receipt) in &history {
        assert!(
            presenter
                .journal()
                .resolve(UiRouteStamp::from_receipt(*receipt))
                .is_err()
        );
    }
    assert!(presenter.is_quiescent());

    let (mut compiler, mut replacement) = fixture(SURFACE + 1, SLOT + 1);
    let next = frame(
        &mut compiler,
        SURFACE + 1,
        SLOT + 1,
        1,
        UiVisual::solid(WHITE),
    );
    let receipt = present(&mut replacement, next);
    assert_eq!(receipt.surface_id(), UiSurfaceId::new(SURFACE + 1).unwrap());
    assert_eq!(receipt.slot_generation().get(), SLOT + 1);
    assert!(
        replacement
            .journal()
            .resolve(UiRouteStamp::from_receipt(history[3].1))
            .is_err()
    );
    assert!(presenter.journal().latest(surface).is_err());
}

#[test]
fn deferred_retry_keeps_the_exact_attempt_packet_and_required_backend_generation() {
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let candidate = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
    let original = submitted(
        presenter
            .prepare(Arc::clone(&candidate), &UiPreparedImages::new())
            .unwrap(),
    );
    let identity = original.submission().identity();
    let packet = Arc::clone(original.submission().packet());
    let generation = original.submission().backend_generation();
    let message = "the CPU backend temporarily defers this target";
    presenter
        .return_outcome(original.deferred(message.to_owned()))
        .unwrap();
    let events = presenter.drain().unwrap();
    let [
        UiSurfacePresentationEvent::Deferred {
            attempt,
            message: actual,
        },
    ] = events.as_slice()
    else {
        panic!("expected the original deferred attempt: {events:?}");
    };
    assert_eq!(attempt.backend_submission(), Some(identity));
    assert_eq!(actual, message);
    assert!(!presenter.is_quiescent());
    assert!(
        presenter
            .journal()
            .latest(candidate.environment().surface_id())
            .is_err()
    );
    let newer = frame(&mut compiler, SURFACE, SLOT, 2, UiVisual::solid(WHITE));
    assert!(presenter.prepare(newer, &UiPreparedImages::new()).is_err());

    let retry = original_submission(presenter.retry().unwrap());
    assert_eq!(retry.submission().identity(), identity);
    assert_eq!(
        retry.submission().required_backend_generation(),
        Some(generation),
    );
    assert!(Arc::ptr_eq(retry.submission().packet(), &packet));
    assert!(presenter.retry().is_err());
    assert!(presenter.drain().is_err());
    let wrong_generation = UiPresentationBackendGeneration::new(
        generation.backend_instance_id(),
        generation.device_epoch() + 1,
    )
    .unwrap();
    let rejected = bind_backend_submission_batch(vec![retry], wrong_generation).unwrap_err();
    assert_eq!(rejected.admission_id(), identity.admission_id());
    assert_eq!(rejected.required(), generation);
    assert_eq!(rejected.actual(), wrong_generation);
    let originals = rejected.into_submissions();
    assert_eq!(originals.len(), 1);
    assert_eq!(originals[0].submission().identity(), identity);
    assert!(Arc::ptr_eq(originals[0].submission().packet(), &packet));
    assert!(presenter.drain().is_err());

    let mut bound = bind_backend_submission_batch(originals, generation).unwrap();
    presenter
        .return_outcome(bound.pop().unwrap().presented())
        .unwrap();
    let receipt = presented_receipt(&presenter.drain().unwrap());
    assert!(identity.matches_receipt(receipt));
    assert!(Arc::ptr_eq(
        presenter
            .journal()
            .resolve(UiRouteStamp::from_receipt(receipt))
            .unwrap()
            .frame(),
        &candidate,
    ));
    assert!(presenter.is_quiescent());
}

#[test]
fn retirement_revokes_routes_before_a_late_presented_outcome_is_discarded() {
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let first = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
    let receipt = present(&mut presenter, first);
    let route = UiRouteStamp::from_receipt(receipt);
    let candidate = frame(&mut compiler, SURFACE, SLOT, 2, UiVisual::solid(WHITE));
    let late = submitted(
        presenter
            .prepare(Arc::clone(&candidate), &UiPreparedImages::new())
            .unwrap(),
    );
    let identity = late.submission().identity();

    let token = presenter.retire(19).unwrap().unwrap();
    assert_eq!(token.surface_id(), receipt.surface_id());
    assert_eq!(token.slot_generation(), receipt.slot_generation());
    assert_eq!(token.retirement_generation().get(), 19);
    assert!(presenter.journal().resolve(route).is_err());
    assert!(presenter.journal().latest(receipt.surface_id()).is_err());
    assert!(!presenter.is_quiescent());
    assert!(
        presenter
            .prepare(candidate, &UiPreparedImages::new())
            .is_err()
    );
    assert!(presenter.retry().is_err());
    assert!(presenter.drain().is_err());

    presenter.return_outcome(late.presented()).unwrap();
    assert!(!presenter.is_quiescent());
    assert!(presenter.journal().resolve(route).is_err());
    let events = presenter.drain().unwrap();
    let [
        UiSurfacePresentationEvent::Discarded {
            attempt: Some(attempt),
        },
    ] = events.as_slice()
    else {
        panic!("late acceptance must settle without publishing a route: {events:?}");
    };
    assert_eq!(attempt.backend_submission(), Some(identity));
    assert!(presenter.journal().resolve(route).is_err());
    assert!(presenter.journal().latest(receipt.surface_id()).is_err());
    assert!(presenter.is_quiescent());
    assert!(presenter.drain().unwrap().is_empty());
    assert!(presenter.retire(20).unwrap().is_none());
}

#[test]
fn retirement_ends_local_and_late_deferrals_and_waits_for_dropped_pending_permits() {
    // Closing after a local deferred retry is retained requires no second backend completion.
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let candidate = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
    let deferred = submitted(
        presenter
            .prepare(candidate, &UiPreparedImages::new())
            .unwrap(),
    );
    presenter
        .return_outcome(deferred.deferred("retry later".into()))
        .unwrap();
    assert!(matches!(
        presenter.drain().unwrap().as_slice(),
        [UiSurfacePresentationEvent::Deferred { .. }]
    ));
    assert!(!presenter.is_quiescent());
    presenter.retire(30).unwrap().unwrap();
    assert!(presenter.is_quiescent());
    assert!(presenter.retry().is_err());
    assert!(presenter.drain().unwrap().is_empty());

    for abandon in [false, true] {
        let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
        let candidate = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
        let late = submitted(
            presenter
                .prepare(candidate, &UiPreparedImages::new())
                .unwrap(),
        );
        let attempt = late.submission().identity();
        presenter.retire(31).unwrap().unwrap();
        assert!(!presenter.is_quiescent());
        if abandon {
            drop(late);
        } else {
            presenter
                .return_outcome(late.deferred("cancelled after retirement".into()))
                .unwrap();
        }
        assert!(
            !presenter.is_quiescent(),
            "returned results must still be drained"
        );
        assert!(matches!(presenter.drain().unwrap().as_slice(),
            [UiSurfacePresentationEvent::Discarded { attempt: Some(identity) }]
                if identity.backend_submission() == Some(attempt)));
        assert!(presenter.is_quiescent());
        assert!(
            presenter
                .journal()
                .latest(UiSurfaceId::new(SURFACE).unwrap())
                .is_err()
        );
        assert!(presenter.retry().is_err());
    }
}

#[test]
fn target_and_device_faults_withdraw_routes_but_leave_retirement_generation_with_the_host() {
    for domain in [
        crate::UiPresentationBackendFaultDomain::Attempt,
        crate::UiPresentationBackendFaultDomain::Target,
        crate::UiPresentationBackendFaultDomain::Device,
    ] {
        let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
        let first = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
        let receipt = present(&mut presenter, first);
        let route = UiRouteStamp::from_receipt(receipt);
        let candidate = frame(&mut compiler, SURFACE, SLOT, 2, UiVisual::solid(WHITE));
        let faulty = submitted(
            presenter
                .prepare(candidate, &UiPreparedImages::new())
                .unwrap(),
        );
        presenter
            .return_outcome(faulty.backend_fault(domain, "actual fault domain".into()))
            .unwrap();
        assert!(matches!(presenter.drain().unwrap().as_slice(),
            [UiSurfacePresentationEvent::Terminal(UiPresentationOutcome::BackendFault {
                domain: returned, ..
            })] if *returned == domain));
        assert!(presenter.is_quiescent());
        let next = frame(&mut compiler, SURFACE, SLOT, 3, UiVisual::solid(WHITE));
        if domain == crate::UiPresentationBackendFaultDomain::Attempt {
            assert_eq!(
                presenter.journal().resolve(route).unwrap().receipt(),
                receipt
            );
            let backend = presenter.prepare(next, &UiPreparedImages::new()).unwrap();
            drop(backend);
            presenter.drain().unwrap();
        } else {
            assert!(presenter.journal().resolve(route).is_err());
            assert!(presenter.journal().latest(receipt.surface_id()).is_err());
            assert!(presenter.prepare(next, &UiPreparedImages::new()).is_err());
            assert!(presenter.retry().is_err());
        }
        let retired = presenter.retire(103).unwrap().unwrap();
        assert_eq!(retired.surface_id(), receipt.surface_id());
        assert_eq!(retired.slot_generation(), receipt.slot_generation());
        assert_eq!(retired.retirement_generation().get(), 103);
        assert!(presenter.retire(104).unwrap().is_none());
    }
}

#[test]
fn wrong_surface_and_slot_are_refused_before_any_backend_permit_is_reserved() {
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let wrong_surface = frame(&mut compiler, SURFACE + 1, SLOT, 1, UiVisual::solid(WHITE));
    let wrong_slot = frame(&mut compiler, SURFACE, SLOT + 1, 2, UiVisual::solid(WHITE));
    for invalid in [wrong_surface, wrong_slot] {
        assert!(
            presenter
                .prepare(invalid, &UiPreparedImages::new())
                .is_err()
        );
        assert!(presenter.is_quiescent());
        assert!(presenter.drain().unwrap().is_empty());
        assert!(
            presenter
                .journal()
                .latest(UiSurfaceId::new(SURFACE).unwrap())
                .is_err()
        );
    }

    let valid = frame(&mut compiler, SURFACE, SLOT, 3, UiVisual::solid(WHITE));
    let receipt = present(&mut presenter, valid);
    assert_eq!(receipt.surface_id(), UiSurfaceId::new(SURFACE).unwrap());
    assert_eq!(receipt.slot_generation().get(), SLOT);
    assert!(presenter.is_quiescent());
}

#[test]
fn planner_content_refusal_preserves_the_last_good_route_and_allows_the_next_frame() {
    let (mut compiler, mut presenter) = fixture(SURFACE, SLOT);
    let good = frame(&mut compiler, SURFACE, SLOT, 1, UiVisual::solid(WHITE));
    let receipt = present(&mut presenter, Arc::clone(&good));
    let route = UiRouteStamp::from_receipt(receipt);
    let missing_image = frame(
        &mut compiler,
        SURFACE,
        SLOT,
        2,
        UiVisual::image(UiImageId::new("project://ui/missing.png").unwrap(), WHITE),
    );
    assert!(
        presenter
            .prepare(missing_image, &UiPreparedImages::new())
            .is_err()
    );
    assert!(presenter.is_quiescent());
    assert!(presenter.drain().unwrap().is_empty());
    let target = presenter.journal().resolve(route).unwrap();
    assert_eq!(target.receipt(), receipt);
    assert!(Arc::ptr_eq(target.frame(), &good));
    assert_eq!(
        presenter
            .journal()
            .latest(receipt.surface_id())
            .unwrap()
            .receipt(),
        receipt,
    );

    let next = frame(&mut compiler, SURFACE, SLOT, 3, UiVisual::solid(WHITE));
    let next_receipt = present(&mut presenter, next);
    assert!(next_receipt.presentation_epoch() > receipt.presentation_epoch());
    assert!(Arc::ptr_eq(
        presenter.journal().resolve(route).unwrap().frame(),
        &good,
    ));
    assert!(presenter.is_quiescent());
}
