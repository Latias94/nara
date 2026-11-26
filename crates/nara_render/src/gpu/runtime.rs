//! Finite CPU custody between one managed Runtime and its concrete local GPU owner.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use crate::{FrameStats, RenderBackendStatus, RenderFrame, RenderWindowPacket, WorldCanvas};
use nara_app::{
    App, PluginError, RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeCloseProgress, RuntimeGeneration, RuntimeInstance,
    RuntimeScopeError, RuntimeScopeOutcome, RuntimeState,
};
use nara_ecs::{Resource, World};

#[cfg(feature = "hud-runtime")]
use crate::gpu::frame::WgpuUiSubmissionState;
use crate::gpu::{
    WgpuRenderObservation,
    frame::{WgpuCapturedFrame, WgpuFrameCaptureError, WgpuFramePacketSlot},
    presented::WgpuPresentedFrameReceipt,
};
#[cfg(feature = "hud-runtime")]
use crate::hud::presentation::{UiCompletedBackendOutcome, commit_ui_backend_outcomes};

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
    #[cfg(all(test, feature = "hud-runtime"))]
    drop_gate: Option<Arc<drop_tests::TransferDropGate>>,
}

impl Drop for Transfer {
    fn drop(&mut self) {
        #[cfg(all(test, feature = "hud-runtime"))]
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
    #[cfg(feature = "hud-runtime")]
    pub(crate) ui: Option<WgpuUiSubmissionState>,
}

/// Original move-only capture. Dropping it returns its original logical responsibilities.
#[derive(Debug)]
pub struct WgpuRuntimeCapture {
    pub(crate) pending: PendingFrame,
    #[cfg(feature = "hud-runtime")]
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
        presented_world: Option<crate::runtime_integration::PresentedWorldFrame>,
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
            #[cfg(feature = "hud-runtime")]
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
    presented_world: Option<crate::runtime_integration::PresentedWorldFrame>,
    fault: Option<nara_app::RuntimeFault>,
    #[cfg(feature = "hud-runtime")]
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
    #[error("render runtime resource {type_name} is missing")]
    MissingResource { type_name: &'static str },
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

impl WgpuRuntimeResultFailure {
    fn refused(result: WgpuRuntimeResult, error: WgpuRuntimeExchangeError) -> Self {
        Self {
            error,
            result,
            post_operation_fault: None,
        }
    }
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
                .expect("WGPU runtime capture identity exhausted"),
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
        #[cfg(feature = "hud-runtime")]
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
                #[cfg(all(test, feature = "hud-runtime"))]
                drop_gate: None,
            },
            pending,
            #[cfg(feature = "hud-runtime")]
            completed,
        }))
    }
}

#[derive(Resource)]
pub(crate) struct ConnectionAuthority(Arc<Mutex<TransferState>>);

impl ConnectionAuthority {
    pub(crate) fn is_current(&self, connection: &WgpuRuntimeConnection) -> bool {
        Arc::ptr_eq(&self.0, &connection.owner)
    }
}

fn take_connection_frame(
    world: &mut World,
    runtime: RuntimeGeneration,
) -> Result<Option<WgpuRuntimeCapture>, WgpuRuntimeExchangeError> {
    let connection = world.get_resource::<WgpuRuntimeConnection>().ok_or(
        WgpuRuntimeExchangeError::MissingResource {
            type_name: std::any::type_name::<WgpuRuntimeConnection>(),
        },
    )?;
    if !world
        .get_resource::<ConnectionAuthority>()
        .is_some_and(|original| original.is_current(connection))
    {
        return Err(WgpuRuntimeExchangeError::OwnerChanged);
    }
    world.resource_mut::<WgpuRuntimeConnection>().take(runtime)
}

fn apply_connection_result(
    world: &mut World,
    runtime: RuntimeGeneration,
    result: WgpuRuntimeResult,
    present: bool,
) -> Result<WgpuRuntimeCompletion, WgpuRuntimeResultFailure> {
    let Some(connection) = world.get_resource::<WgpuRuntimeConnection>() else {
        return Err(WgpuRuntimeResultFailure::refused(
            result,
            WgpuRuntimeExchangeError::MissingResource {
                type_name: std::any::type_name::<WgpuRuntimeConnection>(),
            },
        ));
    };
    let identity = result.identity();
    let valid_owner = identity.runtime_generation() == runtime
        && Arc::ptr_eq(&connection.owner, &result.transfer.owner)
        && world
            .get_resource::<ConnectionAuthority>()
            .is_some_and(|original| original.is_current(connection));
    let retiring = valid_owner
        .then(|| {
            connection
                .owner
                .lock()
                .unwrap_or_else(|error| error.into_inner())
        })
        .filter(|owner| owner.in_flight == Some(identity))
        .map(|owner| !present || owner.closing);
    let Some(retiring) = retiring else {
        return Err(WgpuRuntimeResultFailure::refused(
            result,
            WgpuRuntimeExchangeError::OwnerChanged,
        ));
    };
    for (available, type_name) in [
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
        if !available {
            return Err(WgpuRuntimeResultFailure::refused(
                result,
                WgpuRuntimeExchangeError::MissingResource { type_name },
            ));
        }
    }
    #[cfg(feature = "hud-runtime")]
    let mut result = result;
    #[cfg(feature = "hud-runtime")]
    {
        if retiring
            && let Some(receiver) = crate::hud::presentation::backend_outcome_receiver(world)
        {
            receiver.begin_close();
        }
        if !result.completed.is_empty()
            && let Err(completed) =
                commit_ui_backend_outcomes(world, std::mem::take(&mut result.completed))
        {
            result.completed = completed;
            return Err(WgpuRuntimeResultFailure::refused(
                result,
                WgpuRuntimeExchangeError::UiResultRefused,
            ));
        }
    }
    let presented = if retiring {
        None
    } else {
        result
            .observation
            .as_ref()
            .and_then(WgpuRenderObservation::presented_frame_this_transaction)
    };
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
                crate::RenderFrameSkipReason::NoRenderableTarget,
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
    Ok(WgpuRuntimeCompletion {
        presented,
        retired: retiring,
    })
}

#[cfg(test)]
#[derive(Resource)]
struct PostCompletionFault(nara_app::RuntimeFault);

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
        #[cfg(feature = "hud-runtime")]
        completed: capture.completed,
    }
}

pub fn take_runtime_frame(
    runtime: &mut RuntimeInstance,
) -> Result<RuntimeScopeOutcome<Option<WgpuRuntimeCapture>>, WgpuRuntimeCaptureFailure> {
    let generation = runtime.generation();
    let RuntimeScopeOutcome {
        output,
        post_operation_fault,
    } = runtime
        .with_driver_scope(|scope| {
            let state = scope.state();
            if !matches!(state, RuntimeState::Running | RuntimeState::Paused) {
                return Err(RuntimeScopeError::Unavailable { state }.into());
            }
            scope.with_world_mut(|world| take_connection_frame(world, generation))
        })
        .map_err(|error| WgpuRuntimeCaptureFailure {
            error: WgpuRuntimeExchangeError::Scope(error),
            post_operation_fault: None,
        })?;
    match output {
        Ok(output) => Ok(RuntimeScopeOutcome {
            output,
            post_operation_fault,
        }),
        Err(error) => Err(WgpuRuntimeCaptureFailure {
            error,
            post_operation_fault,
        }),
    }
}

pub fn complete_runtime_frame(
    runtime: &mut RuntimeInstance,
    result: WgpuRuntimeResult,
) -> Result<RuntimeScopeOutcome<WgpuRuntimeCompletion>, WgpuRuntimeResultFailure> {
    let generation = runtime.generation();
    let mut pending_result = Some(result);
    let outcome = runtime.with_driver_scope(|scope| {
        let result = pending_result
            .take()
            .expect("the scope takes the result once");
        let present = match scope.state() {
            RuntimeState::Running | RuntimeState::Paused => true,
            RuntimeState::Faulted | RuntimeState::Stopping | RuntimeState::CloseIncomplete => false,
            state => {
                return Err(WgpuRuntimeResultFailure::refused(
                    result,
                    RuntimeScopeError::Unavailable { state }.into(),
                ));
            }
        };
        scope.with_world_mut(|world| {
            let completion = apply_connection_result(world, generation, result, present);
            #[cfg(test)]
            if let Some(fault) = world.get_resource::<PostCompletionFault>()
                && let Some(reporter) = world.get_resource::<nara_app::RuntimeFaultReporter>()
            {
                reporter.report(fault.0.clone());
            }
            completion
        })
    });
    let RuntimeScopeOutcome {
        output,
        post_operation_fault,
    } = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(WgpuRuntimeResultFailure::refused(
                pending_result.expect("a scope rejected before entry retains its result"),
                error.into(),
            ));
        }
    };
    match output {
        Ok(output) => Ok(RuntimeScopeOutcome {
            output,
            post_operation_fault,
        }),
        Err(mut failure) => {
            failure.post_operation_fault = post_operation_fault;
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
    app.register_close_participant(
        RuntimeCloseParticipantId::new("nara.render-wgpu.connection"),
        ConnectionClose(original),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(all(test, feature = "hud-runtime"))]
#[path = "runtime_drop_tests.rs"]
mod drop_tests;
