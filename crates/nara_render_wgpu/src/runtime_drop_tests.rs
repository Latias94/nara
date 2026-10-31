use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, SyncSender, channel, sync_channel},
    },
    thread,
    time::Duration,
};

use nara_app::{
    Plugin, PluginDeclaration, PluginId, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeControl,
    RuntimeControlRequestResult, RuntimeObligationLedger,
};
use nara_ui_render::{
    __private::{UiBackendOutcomeReceiver, UiPresentationOutcomes, backend_outcome_receiver},
    UiBackendSubmissionIdentity, UiPresentationBackendGeneration, UiPresentationOutcome,
};

use super::*;
use crate::{
    frame::{WgpuUiCompletion, complete_ui_submissions_into},
    tests::{admit_ui_attempt, prepared_ui_attempt, ui_outcome_packet},
};

#[derive(Debug)]
pub(super) struct TransferDropGate {
    pub(super) entered: SyncSender<()>,
    pub(super) release: Mutex<Receiver<()>>,
}

struct DropConnectionHarness;

impl Plugin for DropConnectionHarness {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.render-connection-drop"),
            nara_app::PluginCategory::Backend,
        )
        .shutdown_obligations(&[crate::WGPU_BACKEND_SHUTDOWN_OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<WgpuFramePacketSlot>()?;
        install(app)
    }
}

fn capture_with_original_ui_permit() -> (
    RuntimeInstance,
    WgpuRuntimeCapture,
    UiBackendOutcomeReceiver,
    UiBackendSubmissionIdentity,
) {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
        DropConnectionHarness,
    ))
    .unwrap();
    let receiver = backend_outcome_receiver(app.world()).unwrap();
    let submission = admit_ui_attempt(
        &mut app
            .world_mut()
            .unwrap()
            .resource_mut::<UiPresentationOutcomes>(),
        prepared_ui_attempt(ui_outcome_packet(), 41, 3),
    );
    let expected = submission.submission().identity();
    app.world_mut()
        .unwrap()
        .resource_mut::<WgpuRuntimeConnection>()
        .publish(PendingFrame {
            captured: Ok(None),
            frame: RenderFrame {
                index: 17,
                state: nara_render::RenderFrameState::Rendering,
            },
            window: None,
            canvas: None,
            ui: Some(WgpuUiSubmissionState::Prepared(vec![submission])),
        });
    let mut runtime = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
    let taken = take_runtime_frame(&mut runtime).unwrap();
    assert!(taken.post_operation_fault.is_none());
    let capture = taken.output.unwrap();
    assert_eq!(receiver.pending_backend_count(), 1);
    (runtime, capture, receiver, expected)
}

fn transfer_drop_gate() -> (Arc<TransferDropGate>, Receiver<()>, Sender<()>) {
    let (entered, entered_receiver) = sync_channel(0);
    let (release_sender, release) = channel();
    (
        Arc::new(TransferDropGate {
            entered,
            release: Mutex::new(release),
        }),
        entered_receiver,
        release_sender,
    )
}

fn assert_original_close_waits_for_drop<T: Send + 'static>(
    mut runtime: RuntimeInstance,
    receiver: UiBackendOutcomeReceiver,
    expected: UiBackendSubmissionIdentity,
    value: T,
    entered: Receiver<()>,
    release: Sender<()>,
) {
    let worker = thread::spawn(move || drop(value));

    // Record the interleaving before asserting, so a regression cannot strand the Drop thread.
    let entered_result = entered.recv_timeout(Duration::from_secs(10));
    let pending_at_gate = receiver.pending_backend_count();
    let returned_at_gate = receiver.drain();
    let stop_request = runtime.request_control(RuntimeControl::Stop);
    for _ in 0..20 {
        let _ = runtime.drive(Duration::ZERO);
        if matches!(
            runtime.state(),
            RuntimeState::CloseIncomplete | RuntimeState::Stopped
        ) {
            break;
        }
    }
    let state_at_gate = runtime.state();
    let original_close_pending =
        runtime
            .close_evidence()
            .pending_participants()
            .iter()
            .any(|pending| {
                pending.participant()
                    == RuntimeCloseParticipantId::new("nara.render-wgpu.connection")
            });
    let transfer_still_in_flight = runtime
        .world()
        .resource::<WgpuRuntimeConnection>()
        .owner
        .lock()
        .unwrap()
        .in_flight
        .is_some();

    let release_result = release.send(());
    let worker_result = worker.join();
    let retry_request = runtime.request_control(RuntimeControl::RetryClose);
    for _ in 0..20 {
        if runtime.state() == RuntimeState::Stopped {
            break;
        }
        let _ = runtime.drive(Duration::ZERO);
    }
    let stopped_after_drop = runtime.state() == RuntimeState::Stopped;
    let no_pending_close_after_drop = runtime.close_evidence().pending_participants().is_empty();
    let mut retirement = runtime.begin_retirement();
    let retired =
        (0..20).any(|_| retirement.drive_retirement() == RuntimeCandidateRetirementState::Retired);

    entered_result.expect("Drop must reach the transfer gate");
    release_result.expect("the blocked original transfer must receive its release");
    worker_result.expect("dropping the original CPU value must not panic");
    assert_eq!(
        pending_at_gate, 0,
        "UI permits must return before transfer retirement"
    );
    let returned = returned_at_gate.unwrap();
    assert!(matches!(
        returned.as_slice(),
        [UiPresentationOutcome::Aborted { attempt, .. }]
            if attempt.backend_submission() == Some(expected)
    ));
    assert!(matches!(
        stop_request,
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(state_at_gate, RuntimeState::CloseIncomplete);
    assert!(original_close_pending);
    assert!(transfer_still_in_flight);
    assert!(matches!(
        retry_request,
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert!(stopped_after_drop);
    assert!(no_pending_close_after_drop);
    assert!(retired);
    assert_eq!(receiver.pending_backend_count(), 0);
    assert!(
        receiver.drain().unwrap().is_empty(),
        "the fallback must return exactly once"
    );
}

#[test]
fn capture_drop_returns_original_ui_permit_before_releasing_its_close_barrier() {
    let (runtime, mut capture, receiver, expected) = capture_with_original_ui_permit();
    let (gate, entered, release) = transfer_drop_gate();
    capture.transfer.drop_gate = Some(gate);

    assert_original_close_waits_for_drop(runtime, receiver, expected, capture, entered, release);
}

#[test]
fn result_drop_returns_original_ui_permit_before_releasing_its_close_barrier() {
    let (runtime, mut capture, receiver, expected) = capture_with_original_ui_permit();
    let mut submissions = capture.pending.ui.take().unwrap();
    submissions
        .bind_backend(UiPresentationBackendGeneration::new(17, 3).unwrap())
        .unwrap();
    complete_ui_submissions_into(
        submissions,
        WgpuUiCompletion::Presented,
        &mut capture.completed,
    );
    assert_eq!(capture.completed.len(), 1);
    assert_eq!(receiver.pending_backend_count(), 1);
    let frame = capture.pending.frame;
    let mut result = capture.into_result(
        frame,
        FrameStats::default(),
        RenderBackendStatus::default(),
        WgpuRenderObservation::default(),
        None,
        None,
    );
    let (gate, entered, release) = transfer_drop_gate();
    result.transfer.drop_gate = Some(gate);

    assert_original_close_waits_for_drop(runtime, receiver, expected, result, entered, release);
}
