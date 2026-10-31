//! Finite CPU custody between one managed Runtime and its concrete local GPU owner.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use nara_app::{
    __RuntimeDriverPort, App, PluginError, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    RuntimeGeneration, RuntimeInstance, RuntimeScopeError, RuntimeScopeOutcome, RuntimeState,
    RuntimeWorldAccessError,
};
use nara_ecs::{Resource, World};
use nara_render::{FrameStats, RenderBackendStatus, RenderFrame, RenderWindowPacket, WorldCanvas};

#[cfg(feature = "ui-submitter")]
use crate::frame::WgpuUiSubmissionState;
use crate::{
    WgpuRenderObservation,
    frame::{WgpuCapturedFrame, WgpuFrameCaptureError, WgpuFramePacketSlot},
    presented::WgpuPresentedFrameReceipt,
};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::__private::{UiCompletedBackendOutcome, commit_ui_backend_outcomes};

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuRuntimeCaptureIdentity {
    connection: u64,
    runtime: RuntimeGeneration,
    sequence: u64,
}

impl WgpuRuntimeCaptureIdentity {
    pub const fn connection(self) -> u64 {
        self.connection
    }
    pub const fn runtime_generation(self) -> RuntimeGeneration {
        self.runtime
    }
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
}

#[derive(Debug, Default)]
struct TransferState {
    closing: bool,
    in_flight: Option<WgpuRuntimeCaptureIdentity>,
    pending: Option<PendingFrame>,
}

#[derive(Debug)]
struct Transfer {
    identity: WgpuRuntimeCaptureIdentity,
    owner: Arc<Mutex<TransferState>>,
    #[cfg(all(test, feature = "ui-submitter"))]
    drop_gate: Option<Arc<drop_tests::TransferDropGate>>,
}

impl Drop for Transfer {
    fn drop(&mut self) {
        #[cfg(all(test, feature = "ui-submitter"))]
        if let Some(gate) = &self.drop_gate {
            gate.entered.send(()).unwrap();
            gate.release.lock().unwrap().recv().unwrap();
        }
        let mut owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        if owner.in_flight == Some(self.identity) {
            owner.in_flight = None;
        }
    }
}

#[derive(Debug)]
pub(crate) struct PendingFrame {
    pub(crate) captured: Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>,
    pub(crate) frame: RenderFrame,
    pub(crate) window: Option<RenderWindowPacket>,
    pub(crate) canvas: Option<WorldCanvas>,
    #[cfg(feature = "ui-submitter")]
    pub(crate) ui: Option<WgpuUiSubmissionState>,
}

/// Original move-only capture. Dropping it returns its original logical responsibilities.
#[derive(Debug)]
pub struct WgpuRuntimeCapture {
    pub(crate) pending: PendingFrame,
    #[cfg(feature = "ui-submitter")]
    pub(crate) completed: Vec<UiCompletedBackendOutcome>,
    // Original obligations must finish dropping before the logical in-flight barrier releases.
    transfer: Transfer,
}

impl WgpuRuntimeCapture {
    pub fn identity(&self) -> WgpuRuntimeCaptureIdentity {
        self.transfer.identity
    }

    pub(crate) fn allows_execution(&self) -> bool {
        let owner = self
            .transfer
            .owner
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        !owner.closing && owner.in_flight == Some(self.identity())
    }

    pub(crate) fn into_result(
        self,
        frame: RenderFrame,
        stats: FrameStats,
        status: RenderBackendStatus,
        observation: WgpuRenderObservation,
        presented_world: Option<nara_render::__private::PresentedWorldFrame>,
        fault: Option<nara_app::RuntimeFault>,
    ) -> WgpuRuntimeResult {
        WgpuRuntimeResult {
            transfer: self.transfer,
            frame,
            stats,
            status: Some(status),
            observation: Some(observation),
            presented_world,
            fault,
            #[cfg(feature = "ui-submitter")]
            completed: self.completed,
        }
    }
}

/// Original result custody survives a refused application or a replaced World ingress.
#[derive(Debug)]
pub struct WgpuRuntimeResult {
    frame: RenderFrame,
    stats: FrameStats,
    status: Option<RenderBackendStatus>,
    observation: Option<WgpuRenderObservation>,
    presented_world: Option<nara_render::__private::PresentedWorldFrame>,
    fault: Option<nara_app::RuntimeFault>,
    #[cfg(feature = "ui-submitter")]
    completed: Vec<UiCompletedBackendOutcome>,
    // This guard is last for the same reason as the original capture's guard.
    transfer: Transfer,
}

impl WgpuRuntimeResult {
    pub fn identity(&self) -> WgpuRuntimeCaptureIdentity {
        self.transfer.identity
    }
    pub fn observation(&self) -> Option<&WgpuRenderObservation> {
        self.observation.as_ref()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WgpuRuntimeCompletion {
    pub presented: Option<WgpuPresentedFrameReceipt>,
    pub retired: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WgpuRuntimeExchangeError {
    #[error(transparent)]
    Scope(#[from] RuntimeScopeError),
    #[error(transparent)]
    Port(#[from] RuntimeWorldAccessError),
    #[error("the original render connection is unavailable or replaced")]
    OwnerChanged,
    #[error("the Runtime capture sequence is exhausted")]
    SequenceExhausted,
    #[error("pre-submit render result storage could not be reserved")]
    ResultCapacity,
    #[error("the original UI result receiver refused its completion batch")]
    UiResultRefused,
}

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct WgpuRuntimeCaptureFailure {
    #[source]
    pub error: WgpuRuntimeExchangeError,
    pub post_operation_fault: Option<nara_app::RuntimeFault>,
}

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct WgpuRuntimeResultFailure {
    #[source]
    pub error: WgpuRuntimeExchangeError,
    pub result: WgpuRuntimeResult,
    pub post_operation_fault: Option<nara_app::RuntimeFault>,
}

#[derive(Debug, Resource)]
pub struct WgpuRuntimeConnection {
    incarnation: u64,
    owner: Arc<Mutex<TransferState>>,
    next_sequence: u64,
}

impl Default for WgpuRuntimeConnection {
    fn default() -> Self {
        Self {
            incarnation: NEXT_CONNECTION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .unwrap_or_else(|_| std::process::abort()),
            owner: Arc::new(Mutex::new(TransferState::default())),
            next_sequence: 1,
        }
    }
}

impl WgpuRuntimeConnection {
    pub(crate) fn can_capture_from(&self, authority: &ConnectionAuthority) -> bool {
        authority.is_current(self) && self.can_capture()
    }

    pub(crate) fn can_capture(&self) -> bool {
        let state = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        !state.closing && state.in_flight.is_none() && state.pending.is_none()
    }

    pub(crate) fn publish(&mut self, pending: PendingFrame) {
        debug_assert!(self.can_capture());
        self.owner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending = Some(pending);
    }

    fn take(
        &mut self,
        runtime: RuntimeGeneration,
    ) -> Result<Option<WgpuRuntimeCapture>, WgpuRuntimeExchangeError> {
        let mut owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        if owner.closing || owner.in_flight.is_some() || owner.pending.is_none() {
            return Ok(None);
        }
        let sequence = self.next_sequence;
        let next = sequence
            .checked_add(1)
            .ok_or(WgpuRuntimeExchangeError::SequenceExhausted)?;
        #[cfg(feature = "ui-submitter")]
        let completed = {
            let count = owner
                .pending
                .as_ref()
                .and_then(|pending| pending.ui.as_ref())
                .map_or(0, WgpuUiSubmissionState::submission_count);
            let mut completed = Vec::new();
            completed
                .try_reserve_exact(count)
                .map_err(|_| WgpuRuntimeExchangeError::ResultCapacity)?;
            completed
        };
        let identity = WgpuRuntimeCaptureIdentity {
            connection: self.incarnation,
            runtime,
            sequence,
        };
        let pending = owner
            .pending
            .take()
            .expect("capture admission checked the original pending frame");
        self.next_sequence = next;
        owner.in_flight = Some(identity);
        Ok(Some(WgpuRuntimeCapture {
            transfer: Transfer {
                identity,
                owner: Arc::clone(&self.owner),
                #[cfg(all(test, feature = "ui-submitter"))]
                drop_gate: None,
            },
            pending,
            #[cfg(feature = "ui-submitter")]
            completed,
        }))
    }
}

/// Original requests are constructed only by the Runtime exchange owner.
///
/// ```compile_fail
/// use nara_app::{RuntimeGeneration, __RuntimeDriverPort};
/// use nara_render_wgpu::__private::WgpuRuntimeConnection;
/// type Input = <WgpuRuntimeConnection as __RuntimeDriverPort>::Input;
/// fn forge(generation: RuntimeGeneration) -> Input {
///     Input::Take(generation)
/// }
/// ```
pub struct ConnectionInput(ConnectionOperation);

enum ConnectionOperation {
    Take(RuntimeGeneration),
    Return(WgpuRuntimeResult, bool),
}

pub enum ConnectionOutput {
    Taken(Result<Option<WgpuRuntimeCapture>, WgpuRuntimeExchangeError>),
    Returned(Result<WgpuRuntimeCompletion, WgpuRuntimeResultFailure>),
}

#[derive(Resource)]
pub(crate) struct ConnectionAuthority(Arc<Mutex<TransferState>>);

impl ConnectionAuthority {
    pub(crate) fn is_current(&self, connection: &WgpuRuntimeConnection) -> bool {
        Arc::ptr_eq(&self.0, &connection.owner)
    }
}

impl __RuntimeDriverPort for WgpuRuntimeConnection {
    type Input = ConnectionInput;
    type Output = ConnectionOutput;

    fn accepts_driver_input(state: RuntimeState, input: &Self::Input) -> bool {
        match &input.0 {
            ConnectionOperation::Take(_) => {
                matches!(state, RuntimeState::Running | RuntimeState::Paused)
            }
            ConnectionOperation::Return(_, true) => {
                matches!(state, RuntimeState::Running | RuntimeState::Paused)
            }
            ConnectionOperation::Return(_, false) => matches!(
                state,
                RuntimeState::Faulted | RuntimeState::Stopping | RuntimeState::CloseIncomplete
            ),
        }
    }

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        unreachable!("render exchange uses its registered concrete World operation")
    }
}

fn apply_connection(
    world: &mut World,
    input: ConnectionInput,
) -> Result<ConnectionOutput, (RuntimeWorldAccessError, ConnectionInput)> {
    Ok(match input.0 {
        ConnectionOperation::Take(runtime) => {
            if !world
                .get_resource::<ConnectionAuthority>()
                .is_some_and(|original| {
                    original.is_current(world.resource::<WgpuRuntimeConnection>())
                })
            {
                return Ok(ConnectionOutput::Taken(Err(
                    WgpuRuntimeExchangeError::OwnerChanged,
                )));
            }
            ConnectionOutput::Taken(world.resource_mut::<WgpuRuntimeConnection>().take(runtime))
        }
        ConnectionOperation::Return(result, present) => {
            #[cfg(feature = "ui-submitter")]
            let mut result = result;
            let connection = world.resource::<WgpuRuntimeConnection>();
            let valid = Arc::ptr_eq(&connection.owner, &result.transfer.owner)
                && world
                    .get_resource::<ConnectionAuthority>()
                    .is_some_and(|original| Arc::ptr_eq(&original.0, &connection.owner))
                && connection
                    .owner
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .in_flight
                    == Some(result.identity());
            if !valid {
                return Ok(ConnectionOutput::Returned(Err(WgpuRuntimeResultFailure {
                    error: WgpuRuntimeExchangeError::OwnerChanged,
                    result,
                    post_operation_fault: None,
                })));
            }
            for (present, name) in [
                (
                    world.contains_resource::<RenderFrame>(),
                    std::any::type_name::<RenderFrame>(),
                ),
                (
                    world.contains_resource::<FrameStats>(),
                    std::any::type_name::<FrameStats>(),
                ),
                (
                    world.contains_resource::<RenderBackendStatus>(),
                    std::any::type_name::<RenderBackendStatus>(),
                ),
                (
                    world.contains_resource::<WgpuRenderObservation>(),
                    std::any::type_name::<WgpuRenderObservation>(),
                ),
            ] {
                if !present {
                    return Ok(ConnectionOutput::Returned(Err(WgpuRuntimeResultFailure {
                        error: WgpuRuntimeExchangeError::Port(
                            RuntimeWorldAccessError::MissingResource { type_name: name },
                        ),
                        result,
                        post_operation_fault: None,
                    })));
                }
            }
            let retiring = !present
                || connection
                    .owner
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .closing;
            #[cfg(feature = "ui-submitter")]
            {
                if retiring
                    && let Some(receiver) =
                        nara_ui_render::__private::backend_outcome_receiver(world)
                {
                    receiver.begin_close();
                }
                if !result.completed.is_empty()
                    && let Err(completed) =
                        commit_ui_backend_outcomes(world, std::mem::take(&mut result.completed))
                {
                    result.completed = completed;
                    return Ok(ConnectionOutput::Returned(Err(WgpuRuntimeResultFailure {
                        error: WgpuRuntimeExchangeError::UiResultRefused,
                        result,
                        post_operation_fault: None,
                    })));
                }
            }
            let presented = (!retiring)
                .then(|| {
                    result
                        .observation
                        .as_ref()
                        .and_then(WgpuRenderObservation::presented_frame_this_transaction)
                })
                .flatten();
            if world.resource::<RenderFrame>().index == result.frame.index {
                *world.resource_mut::<RenderFrame>() = result.frame;
                *world.resource_mut::<FrameStats>() = result.stats;
            }
            if let Some(status) = result.status {
                *world.resource_mut::<RenderBackendStatus>() = status;
            } else {
                world
                    .resource_mut::<RenderBackendStatus>()
                    .mark_skipped_with_message(
                        result.frame.index,
                        nara_render::RenderFrameSkipReason::NoRenderableTarget,
                        "the original capture was cancelled before native execution",
                    );
            }
            if let Some(observation) = result.observation {
                *world.resource_mut::<WgpuRenderObservation>() = observation;
            }
            if !retiring && let Some(presented) = result.presented_world {
                world.insert_resource(presented);
            }
            if let Some(fault) = result.fault
                && let Some(reporter) = world.get_resource::<nara_app::RuntimeFaultReporter>()
            {
                reporter.report(fault);
            }
            drop(result.transfer);
            ConnectionOutput::Returned(Ok(WgpuRuntimeCompletion {
                presented,
                retired: retiring,
            }))
        }
    })
}

/// Converts an unexecuted capture to its original logical result after a Runtime fault.
/// Original UI permissions return Aborted through their retained receivers; no GPU fact is issued.
pub fn cancel_runtime_frame(capture: WgpuRuntimeCapture) -> WgpuRuntimeResult {
    let mut frame = capture.pending.frame;
    frame.mark_skipped();
    WgpuRuntimeResult {
        transfer: capture.transfer,
        frame,
        stats: FrameStats::default(),
        status: None,
        observation: None,
        presented_world: None,
        fault: None,
        #[cfg(feature = "ui-submitter")]
        completed: capture.completed,
    }
}

pub fn take_runtime_frame(
    runtime: &mut RuntimeInstance,
) -> Result<RuntimeScopeOutcome<Option<WgpuRuntimeCapture>>, WgpuRuntimeCaptureFailure> {
    let outcome = runtime
        .__with_owned_driver_port::<WgpuRuntimeConnection>(ConnectionInput(
            ConnectionOperation::Take(runtime.generation()),
        ))
        .map_err(|(error, _)| WgpuRuntimeCaptureFailure {
            error: WgpuRuntimeExchangeError::Scope(error),
            post_operation_fault: None,
        })?;
    let output = outcome
        .output
        .map_err(|(error, _)| WgpuRuntimeCaptureFailure {
            error: WgpuRuntimeExchangeError::Port(error),
            post_operation_fault: outcome.post_operation_fault.clone(),
        })?;
    let ConnectionOutput::Taken(output) = output else {
        unreachable!("the exact take operation returns a capture")
    };
    Ok(RuntimeScopeOutcome {
        output: output.map_err(|error| WgpuRuntimeCaptureFailure {
            error,
            post_operation_fault: outcome.post_operation_fault.clone(),
        })?,
        post_operation_fault: outcome.post_operation_fault,
    })
}

pub fn complete_runtime_frame(
    runtime: &mut RuntimeInstance,
    result: WgpuRuntimeResult,
) -> Result<RuntimeScopeOutcome<WgpuRuntimeCompletion>, WgpuRuntimeResultFailure> {
    let present = matches!(
        runtime.state(),
        RuntimeState::Running | RuntimeState::Paused
    );
    let outcome = match runtime.__with_owned_driver_port::<WgpuRuntimeConnection>(ConnectionInput(
        ConnectionOperation::Return(result, present),
    )) {
        Ok(outcome) => outcome,
        Err((error, ConnectionInput(ConnectionOperation::Return(result, _)))) => {
            return Err(WgpuRuntimeResultFailure {
                error: WgpuRuntimeExchangeError::Scope(error),
                result,
                post_operation_fault: None,
            });
        }
        Err(_) => unreachable!("refused application retains the exact result"),
    };
    let output = match outcome.output {
        Ok(output) => output,
        Err((error, ConnectionInput(ConnectionOperation::Return(result, _)))) => {
            return Err(WgpuRuntimeResultFailure {
                error: WgpuRuntimeExchangeError::Port(error),
                result,
                post_operation_fault: outcome.post_operation_fault,
            });
        }
        Err(_) => unreachable!("refused application retains the exact result"),
    };
    let ConnectionOutput::Returned(output) = output else {
        unreachable!("the exact return operation applies a result")
    };
    match output {
        Ok(output) => Ok(RuntimeScopeOutcome {
            output,
            post_operation_fault: outcome.post_operation_fault,
        }),
        Err(mut failure) => {
            failure.post_operation_fault = outcome.post_operation_fault;
            Err(failure)
        }
    }
}

struct ConnectionClose(Arc<Mutex<TransferState>>);

impl ConnectionClose {
    fn drive(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let complete = {
            let mut original = self.0.lock().unwrap_or_else(|error| error.into_inner());
            original.closing = true;
            original.pending = None;
            original.in_flight.is_none()
        };
        if let Some(mut slot) = context
            .get_resource_mut::<WgpuFramePacketSlot>()
            .map_err(|_| {
                RuntimeCloseParticipantError::retryable("render-capture-builder-unavailable")
            })?
        {
            slot.close();
        }
        Ok(if complete {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

impl RuntimeCloseParticipant for ConnectionClose {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.drive(context)
    }
    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.drive(context)
    }
}

pub(crate) fn install(app: &mut App) -> Result<(), PluginError> {
    app.insert_resource(WgpuRuntimeConnection::default())?;
    let original = Arc::clone(&app.world().resource::<WgpuRuntimeConnection>().owner);
    app.insert_resource(ConnectionAuthority(Arc::clone(&original)))?;
    app.__register_driver_port::<WgpuRuntimeConnection>(apply_connection)?;
    app.register_plugin_runtime_close_participant(
        crate::WGPU_BACKEND_SHUTDOWN_OBLIGATION,
        RuntimeCloseParticipantId::new("nara.render-wgpu.connection"),
        ConnectionClose(original),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(all(test, feature = "ui-submitter"))]
#[path = "runtime_drop_tests.rs"]
mod drop_tests;
