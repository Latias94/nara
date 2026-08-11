use std::time::Duration;

use nara::{input::KeyCode, ui::UiPresentationReceipt};
use nara_reference_game::{
    WaveOutcome, WaveRetryPhase, WaveRetryRejection, WaveRetryStatus, WaveSnapshot,
    bundled_wave_commands,
};

use super::{
    ProductUiCapture, UiPhysicalTarget, capture,
    interaction::{
        drive_fixed, drive_to_terminal, inject_keyboard, inject_pointer_click, release_keyboard,
    },
    presenter::{
        ComponentPresentationState, ComponentRetryCommandTrace,
        retry_replacement_rejection_fixture_plugin,
    },
    product_runtime, stop_runtime,
};

#[derive(Debug, PartialEq)]
pub struct SingleRuntimeRetryTrajectory {
    pub running: ProductUiCapture,
    pub completed: ProductUiCapture,
    pub first_pending: ProductUiCapture,
    pub pending_after_reactivation: ProductUiCapture,
    pub rejected: ProductUiCapture,
    pub second_pending: ProductUiCapture,
    pub applied: ProductUiCapture,
    pub first_activation_receipt: UiPresentationReceipt,
    pub second_activation_receipt: UiPresentationReceipt,
    pub retry_batches_after_first_activation: Vec<usize>,
    pub retry_batches_after_pending_reactivation: Vec<usize>,
    pub retry_command_batches: Vec<usize>,
    pub retry_trace_overflowed: bool,
}

pub fn capture_single_runtime_retry_trajectory(
    target: UiPhysicalTarget,
) -> SingleRuntimeRetryTrajectory {
    let mut runtime = product_runtime(
        target,
        retry_replacement_rejection_fixture_plugin(),
        bundled_wave_commands(),
    );
    drive_fixed(&mut runtime);
    assert_eq!(
        runtime.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Running
    );
    let running = capture(&runtime, target);

    drive_to_terminal(&mut runtime);
    assert_eq!(
        runtime.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Completed
    );
    let completed = capture(&runtime, target);
    let first_activation_receipt = runtime
        .world()
        .resource::<ComponentPresentationState>()
        .latest_receipt()
        .expect("the focused terminal frame should retain an exact receipt");

    inject_keyboard(&mut runtime, KeyCode::Enter);
    runtime
        .drive(Duration::ZERO)
        .expect("the first Retry activation frame should drive");
    drive_fixed(&mut runtime);
    let first_pending_retry = *runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(first_pending_retry.phase(), WaveRetryPhase::Pending);
    let first_ui_action_sequence = first_pending_retry
        .pending_ui_action_sequence()
        .expect("the pending Retry should retain its exact UI action cause");
    let first_pending = capture(&runtime, target);
    let retry_batches_after_first_activation = runtime
        .world()
        .resource::<ComponentRetryCommandTrace>()
        .batches()
        .to_vec();

    release_keyboard(&mut runtime, KeyCode::Enter);
    runtime
        .drive(Duration::ZERO)
        .expect("the first Retry release frame should drive");
    inject_pointer_click(&mut runtime);
    runtime
        .drive(Duration::ZERO)
        .expect("the pending Retry reactivation frame should drive");
    assert_eq!(
        runtime.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    let pending_after_reactivation = capture(&runtime, target);
    let retry_batches_after_pending_reactivation = runtime
        .world()
        .resource::<ComponentRetryCommandTrace>()
        .batches()
        .to_vec();

    drive_fixed(&mut runtime);
    assert_replacement_rejected(&runtime);
    drive_fixed(&mut runtime);
    assert_replacement_rejected(&runtime);
    assert_eq!(
        runtime
            .world()
            .resource::<WaveRetryStatus>()
            .pending_ui_action_sequence(),
        None,
        "a rejected replacement must clear its pending UI action cause"
    );
    let rejected = capture(&runtime, target);
    let second_activation_receipt = runtime
        .world()
        .resource::<ComponentPresentationState>()
        .latest_receipt()
        .expect("the rejected Retry frame should retain an exact receipt");

    inject_keyboard(&mut runtime, KeyCode::Enter);
    runtime
        .drive(Duration::ZERO)
        .expect("the second Retry activation frame should drive");
    drive_fixed(&mut runtime);
    let second_pending_retry = *runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(second_pending_retry.phase(), WaveRetryPhase::Pending);
    let second_ui_action_sequence = second_pending_retry
        .pending_ui_action_sequence()
        .expect("the second pending Retry should retain its exact UI action cause");
    assert!(second_ui_action_sequence > first_ui_action_sequence);
    let second_pending = capture(&runtime, target);

    release_keyboard(&mut runtime, KeyCode::Enter);
    runtime
        .drive(Duration::ZERO)
        .expect("the second Retry release frame should drive");
    drive_fixed(&mut runtime);
    let applied_retry = *runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(applied_retry.phase(), WaveRetryPhase::Applied);
    assert_eq!(
        applied_retry.applied_ui_action_sequence(),
        Some(second_ui_action_sequence),
        "the applied replacement must retain the exact pending UI action cause"
    );
    let applied = capture(&runtime, target);
    let (retry_command_batches, retry_trace_overflowed) = {
        let trace = runtime.world().resource::<ComponentRetryCommandTrace>();
        (trace.batches().to_vec(), trace.overflowed())
    };
    stop_runtime(runtime);

    SingleRuntimeRetryTrajectory {
        running,
        completed,
        first_pending,
        pending_after_reactivation,
        rejected,
        second_pending,
        applied,
        first_activation_receipt,
        second_activation_receipt,
        retry_batches_after_first_activation,
        retry_batches_after_pending_reactivation,
        retry_command_batches,
        retry_trace_overflowed,
    }
}

fn assert_replacement_rejected(runtime: &nara::app::RuntimeInstance) {
    let retry = *runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Idle);
    assert_eq!(
        retry.last_rejection(),
        Some(WaveRetryRejection::ReplacementRejected)
    );
}
