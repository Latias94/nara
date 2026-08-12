use super::*;
use nara_ui_kernel::{UiLocalKey, UiRootKey};

fn retry_target() -> UiKey {
    UiKey::scoped(
        UiRootKey::runtime("probe", "root").unwrap(),
        UiLocalKey::runtime("probe", "retry").unwrap(),
    )
    .unwrap()
}

fn action(sequence: u64) -> ProbeActionEvidence {
    ProbeActionEvidence {
        sequence,
        input_sequence: None,
        surface_id: nara_reference_game::REFERENCE_UI_SURFACE_ID,
        presentation_epoch: 7,
        frame_revision: 9,
        target: retry_target(),
        action: UiActionId::new(nara_reference_game::REFERENCE_RETRY_ACTION).unwrap(),
        source: UiActivationSource::Accessibility,
    }
}

fn arm(evidence: &ProbeEvidence) {
    evidence.observe_terminal_frame("Nara Reference Wave", 12);
    evidence.request_arm(41, vec![42, 7]).unwrap();
    evidence.arm_runtime(4, 9, retry_target()).unwrap();
    evidence.begin_invoke(41, &[42, 7]).unwrap();
}

#[test]
fn exact_platform_trajectory_advances_monotonically() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    evidence.observe_action(action(5)).unwrap();
    evidence.observe_retry_applied(2, Some(5)).unwrap();
    evidence.observe_successor_frame(12).unwrap();
    assert_eq!(evidence.snapshot().phase, ProbePhase::RetryApplied);
    evidence.observe_successor_frame(13).unwrap();
    evidence.observe_successor_tree(41).unwrap();
    evidence.complete_after_journal_barrier(5).unwrap();

    let trace = evidence.snapshot();
    assert!(trace.is_complete());
    assert_eq!(trace.before_wgpu_frame, Some(12));
    assert_eq!(trace.after_wgpu_frame, Some(13));
    assert_eq!(trace.journal_floor, Some(4));
    assert_eq!(trace.journal_barrier, Some(5));
}

#[test]
fn duplicate_retry_action_fails_closed() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    evidence.observe_action(action(5)).unwrap();

    assert!(evidence.observe_action(action(6)).is_err());
    assert!(evidence.snapshot().has_failed());
}

#[test]
fn in_flight_callback_waits_for_successful_invoke_before_advancing() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.observe_action(action(5)).unwrap();
    evidence.observe_retry_applied(2, Some(5)).unwrap();
    evidence.observe_successor_frame(13).unwrap();

    let trace = evidence.snapshot();
    assert_eq!(trace.phase, ProbePhase::InvokeReserved);
    assert!(!trace.is_complete());

    evidence.confirm_invoke().unwrap();
    assert_eq!(evidence.snapshot().phase, ProbePhase::SuccessorPresented);
}

#[test]
fn action_at_or_before_runtime_journal_floor_is_rejected() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();

    assert!(evidence.observe_action(action(4)).is_err());
    assert!(evidence.snapshot().has_failed());
}

#[test]
fn action_from_a_pre_arm_frame_is_rejected() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    let mut mismatched = action(5);
    mismatched.frame_revision = 8;

    assert!(evidence.observe_action(mismatched).is_err());
    assert!(evidence.snapshot().has_failed());
}

#[test]
fn action_must_match_the_runtime_armed_target() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    let mut mismatched = action(5);
    mismatched.target = UiKey::scoped(
        UiRootKey::runtime("probe", "root").unwrap(),
        UiLocalKey::runtime("probe", "other").unwrap(),
    )
    .unwrap();

    assert!(evidence.observe_action(mismatched).is_err());
    assert!(evidence.snapshot().has_failed());
}

#[test]
fn equivalent_successor_frame_may_carry_the_exact_armed_target() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    let mut successor = action(5);
    successor.frame_revision = 10;

    evidence.observe_action(successor).unwrap();
    assert_eq!(evidence.snapshot().phase, ProbePhase::ActionObserved);
}

#[test]
fn applied_retry_must_retain_the_observed_action_sequence() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    evidence.observe_action(action(5)).unwrap();

    assert!(evidence.observe_retry_applied(2, Some(6)).is_err());
    assert!(evidence.snapshot().has_failed());
}

#[test]
fn duplicate_after_completion_invalidates_the_probe() {
    let evidence = ProbeEvidence::default();
    arm(&evidence);
    evidence.confirm_invoke().unwrap();
    evidence.observe_action(action(5)).unwrap();
    evidence.observe_retry_applied(2, Some(5)).unwrap();
    evidence.observe_successor_frame(13).unwrap();
    evidence.observe_successor_tree(41).unwrap();
    evidence.complete_after_journal_barrier(5).unwrap();

    assert!(evidence.observe_action(action(6)).is_err());
    let trace = evidence.snapshot();
    assert!(trace.has_failed());
    assert!(!trace.is_complete());
}
