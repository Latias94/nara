//! Thread-bound runtime ownership, close driving, and quarantine custody.

use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use bevy_ecs::error::ErrorHandler;
use nara_ecs::{
    World,
    observer::Observer,
    schedule::{IntoScheduleConfigs, Schedule, Schedules},
};

use crate::{
    AppExitRequests, CoreStage, FixedTime, FixedUpdateSet, InstalledPlugin,
    InstalledRuntimeAuthorityValidator, PluginFailureReport, PluginHook, PluginId,
    PluginLifecycleState, PluginShutdownError, RealTime, RenderTime, RuntimeFrameStatus,
    RuntimeTimeSettings, StartupStage, VirtualTime,
};

use super::close::{OwnerCloseAttempt, OwnerCloseState, PluginShutdownState};
use super::fault_route::{FaultRouteLease, FaultRouteToken};
use super::{
    DriverScopeAuthorityPolicy, RuntimeCandidateRetirementState, RuntimeCloseCause,
    RuntimeCloseEvidence, RuntimeClosePolicy, RuntimeFault, RuntimeFaultBridgeEpoch,
    RuntimeFaultBridgeIdentity, RuntimeFaultBridgeRevision, RuntimeFaultReporter,
    RuntimeGeneration, RuntimeObligationLedger, RuntimeObligationLedgerError,
    initialize_runtime_fault_bridge, run_runtime_fault_bridge_maintenance,
    runtime_fault_bridge_authority_fault, validate_runtime_fault_bridge_authority,
    with_runtime_system_fault_capture,
};
#[cfg(test)]
use super::{RuntimeAdmissionReservation, allocate_generation};

const MAX_QUARANTINED_RUNTIME_OWNERS_PER_THREAD: usize = 32;
pub(super) const MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS: usize = 128;

static QUARANTINED_RUNTIME_OWNER_COUNT: AtomicUsize = AtomicUsize::new(0);
static TOTAL_QUARANTINED_RUNTIME_OWNERS: AtomicU64 = AtomicU64::new(0);
static TOTAL_REAPED_RUNTIME_OWNERS: AtomicU64 = AtomicU64::new(0);

/// Snapshot of unfinished runtime owners retained by the process quarantine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeQuarantineStatus {
    current_thread_retained: usize,
    current_thread_capacity: usize,
    process_retained: usize,
    process_capacity: usize,
    total_retained: u64,
    total_reaped: u64,
}

impl RuntimeQuarantineStatus {
    #[must_use]
    pub const fn current_thread_retained(self) -> usize {
        self.current_thread_retained
    }

    #[must_use]
    pub const fn current_thread_capacity(self) -> usize {
        self.current_thread_capacity
    }

    #[must_use]
    pub const fn process_retained(self) -> usize {
        self.process_retained
    }

    #[must_use]
    pub const fn process_capacity(self) -> usize {
        self.process_capacity
    }

    #[must_use]
    pub const fn total_retained(self) -> u64 {
        self.total_retained
    }

    #[must_use]
    pub const fn total_reaped(self) -> u64 {
        self.total_reaped
    }
}

/// Returns process-level accounting plus the current owner thread's retained count.
#[must_use]
pub fn runtime_quarantine_status() -> RuntimeQuarantineStatus {
    let current_thread_retained = runtime_quarantine_occupancy();
    RuntimeQuarantineStatus {
        current_thread_retained,
        current_thread_capacity: MAX_QUARANTINED_RUNTIME_OWNERS_PER_THREAD,
        process_retained: QUARANTINED_RUNTIME_OWNER_COUNT.load(Ordering::Acquire),
        process_capacity: MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS,
        total_retained: TOTAL_QUARANTINED_RUNTIME_OWNERS.load(Ordering::Acquire),
        total_reaped: TOTAL_REAPED_RUNTIME_OWNERS.load(Ordering::Acquire),
    }
}

/// Polls each queued runtime owner retained on the current owner thread at most once.
///
/// A close participant panic follows Rust's normal unwind policy. The owner is returned to the
/// quarantine during unwinding, so a caller may inspect the aggregate status or retry from a later
/// owner-thread safe boundary. A thread must drain its quarantine before exiting.
#[must_use]
pub fn drive_runtime_quarantine() -> RuntimeQuarantineStatus {
    let drive_budget = runtime_quarantine_queued_len();
    for _ in 0..drive_budget {
        let Some(state) = pop_runtime_quarantine_owner() else {
            break;
        };
        let mut owner = RuntimeOwner::from_quarantine(state);
        match owner.close_state() {
            RuntimeCandidateRetirementState::Retiring => owner.begin_close(),
            RuntimeCandidateRetirementState::RetirementIncomplete => owner.retry_close(),
            RuntimeCandidateRetirementState::Retired => {}
        }
        owner.drive_close();
        if owner.close_state() == RuntimeCandidateRetirementState::Retired {
            drop(owner);
        } else {
            retain_existing_runtime_owner(owner.take_state());
        }
    }
    runtime_quarantine_status()
}

pub(crate) struct RuntimeOwner {
    state: Option<RuntimeOwnerState>,
    quarantine_accounted: bool,
}

pub(crate) struct RuntimeOwnerState {
    _thread_affinity: PhantomData<Rc<()>>,
    pub(crate) world: World,
    pub(crate) schedules: Schedules,
    pub(crate) plugins: Vec<InstalledPlugin>,
    pub(crate) runtime_fault_reporter: RuntimeFaultReporter,
    pub(crate) managed_runtime_generation: Option<RuntimeGeneration>,
    pub(crate) runtime_error_handler: Option<ErrorHandler>,
    pub(crate) runtime_fault_bridge: Option<RuntimeFaultBridgeIdentity>,
    pub(crate) runtime_authority_validator: Option<InstalledRuntimeAuthorityValidator>,
    pub(crate) simulation_conditions: Vec<fn(&World) -> bool>,
    pub(crate) pending_observers: Vec<Observer>,
    pub(crate) plugin_lifecycle: PluginLifecycleState,
    pub(crate) plugin_failure_report: Option<PluginFailureReport>,
    pub(crate) active_plugin_hook: Option<(PluginId, PluginHook)>,
    pub(crate) started: bool,
    pub(crate) obligations: RuntimeObligationLedger,
    pub(super) fault_route: Option<FaultRouteLease>,
    pub(super) close_policy: RuntimeClosePolicy,
    close_evidence: RuntimeCloseEvidence,
    close_state: OwnerCloseState,
}

impl RuntimeOwner {
    pub(crate) fn new() -> Self {
        Self {
            state: Some(RuntimeOwnerState::new()),
            quarantine_accounted: false,
        }
    }

    fn from_quarantine(state: RuntimeOwnerState) -> Self {
        Self {
            state: Some(state),
            quarantine_accounted: true,
        }
    }

    fn take_state(&mut self) -> RuntimeOwnerState {
        self.state
            .take()
            .expect("runtime owner state transfers exactly once")
    }
}

impl Deref for RuntimeOwner {
    type Target = RuntimeOwnerState;

    fn deref(&self) -> &Self::Target {
        self.state
            .as_ref()
            .expect("runtime owner state remains present while usable")
    }
}

impl DerefMut for RuntimeOwner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
            .as_mut()
            .expect("runtime owner state remains present while usable")
    }
}

impl RuntimeOwnerState {
    pub(super) fn prepend_host_obligations(
        &mut self,
        mut obligations: RuntimeObligationLedger,
    ) -> Result<(), RuntimeObligationLedgerError> {
        let admission = obligations.preflight_append(&self.obligations);
        obligations.append_for_retirement(std::mem::take(&mut self.obligations));
        self.obligations = obligations;
        admission
    }

    #[cfg(test)]
    pub(crate) fn admit_configuration_for_tests(&mut self) {
        if self.managed_runtime_generation.is_some() {
            return;
        }
        let mut reservation = RuntimeAdmissionReservation::try_acquire().unwrap();
        self.fault_route = Some(reservation.take_route().activate(self.reporter().clone()));
        let handler = self.fault_route.as_ref().unwrap().handler();
        self.bind_managed_runtime_error_handler(handler);
        let generation = allocate_generation().unwrap();
        self.managed_runtime_generation = Some(generation);
        self.world.insert_resource(generation);
    }

    fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(RealTime::default());
        world.insert_resource(VirtualTime::default());
        world.insert_resource(RuntimeTimeSettings::default());
        world.insert_resource(FixedTime::default());
        world.insert_resource(RenderTime::default());
        world.insert_resource(RuntimeFrameStatus::default());
        world.insert_resource(AppExitRequests::default());
        let runtime_fault_reporter = RuntimeFaultReporter::new();
        initialize_runtime_fault_bridge(&mut world, runtime_fault_reporter.clone());

        let mut schedules = Schedules::new();
        for stage in StartupStage::ALL {
            schedules.insert(Schedule::new(stage));
        }
        for stage in CoreStage::ALL {
            schedules.insert(Schedule::new(stage));
        }
        schedules.insert(Schedule::new(crate::__private::FrameCleanup));
        schedules
            .get_mut(CoreStage::FixedUpdate)
            .expect("the fixed-update schedule is created above")
            .configure_sets(
                (
                    FixedUpdateSet::Prepare,
                    FixedUpdateSet::Simulate,
                    FixedUpdateSet::Finalize,
                )
                    .chain(),
            );

        Self {
            _thread_affinity: PhantomData,
            world,
            schedules,
            plugins: Vec::new(),
            obligations: RuntimeObligationLedger::new(),
            runtime_fault_reporter,
            fault_route: None,
            close_policy: RuntimeClosePolicy::default(),
            close_evidence: RuntimeCloseEvidence::default(),
            close_state: OwnerCloseState::Open,
            managed_runtime_generation: None,
            runtime_error_handler: None,
            runtime_fault_bridge: None,
            runtime_authority_validator: None,
            simulation_conditions: Vec::new(),
            pending_observers: Vec::new(),
            plugin_lifecycle: PluginLifecycleState::Configuring,
            plugin_failure_report: None,
            active_plugin_hook: None,
            started: false,
        }
    }

    pub(crate) fn simulation_is_ready(&self) -> bool {
        self.simulation_conditions
            .iter()
            .all(|condition| condition(&self.world))
    }

    pub(super) fn begin_close(&mut self) {
        if matches!(self.close_state, OwnerCloseState::Open) {
            self.obligations
                .update_pending_evidence(&mut self.close_evidence);
            self.runtime_fault_reporter.mark_closing();
            self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                deadline: deadline_after(self.close_policy.timeout()),
                plugin_shutdown: PluginShutdownState::Pending,
            });
        }
    }

    pub(super) fn retry_close(&mut self) {
        match self.close_state {
            OwnerCloseState::Open => self.begin_close(),
            OwnerCloseState::Incomplete { plugin_shutdown } => {
                self.runtime_fault_reporter.mark_closing();
                self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                    deadline: deadline_after(self.close_policy.timeout()),
                    plugin_shutdown,
                });
            }
            OwnerCloseState::Closing(_) | OwnerCloseState::Complete => {}
        }
    }

    pub(super) fn finish_close_attempt(&mut self) {
        self.retry_close();
        loop {
            self.drive_close();
            let OwnerCloseState::Closing(attempt) = self.close_state else {
                return;
            };
            let remaining = attempt.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.close_evidence
                    .record(RuntimeCloseCause::DeadlineExceeded);
                self.close_state = OwnerCloseState::Incomplete {
                    plugin_shutdown: attempt.plugin_shutdown,
                };
                return;
            }
            std::thread::sleep(Duration::from_millis(1).min(remaining));
        }
    }

    pub(super) fn drive_close(&mut self) {
        let OwnerCloseState::Closing(mut attempt) = self.close_state else {
            return;
        };

        let bridge_epoch = if self.fault_route.is_some() {
            match self.begin_fault_bridge_epoch() {
                Ok(epoch) => Some(epoch),
                Err(fault) => {
                    self.runtime_fault_reporter.record_canonical(fault);
                    None
                }
            }
        } else {
            None
        };
        let reporter = self.runtime_fault_reporter.clone();
        let fault_route = self.fault_route_token();
        let close_pass = with_runtime_system_fault_capture(fault_route, || {
            if attempt.plugin_shutdown == PluginShutdownState::Pending {
                attempt.plugin_shutdown = match self.shutdown_plugins() {
                    Ok(()) => PluginShutdownState::Complete,
                    Err(PluginShutdownError::Failure(_)) => {
                        self.close_evidence
                            .record(RuntimeCloseCause::PluginShutdown);
                        PluginShutdownState::Complete
                    }
                    Err(PluginShutdownError::HookActive) => PluginShutdownState::Pending,
                };
            }

            let participant_failed = self
                .obligations
                .drive_close_once_with_evidence(&mut self.world, &mut self.close_evidence);
            (attempt, participant_failed)
        });
        self.obligations
            .update_pending_evidence(&mut self.close_evidence);
        let (attempt, participant_failed) = match close_pass {
            Ok(close_pass) => close_pass,
            Err(fault) => {
                reporter.record_canonical(fault);
                return;
            }
        };

        if let Some(epoch) = bridge_epoch
            && let Err(fault) = self.validate_fault_bridge_epoch(epoch)
        {
            self.runtime_fault_reporter.record_canonical(fault);
        }

        let all_complete = self.obligations.is_close_complete();
        if all_complete && attempt.plugin_shutdown == PluginShutdownState::Complete {
            self.runtime_fault_reporter.mark_closed();
            drop(self.fault_route.take());
            self.close_state = OwnerCloseState::Complete;
        } else if participant_failed || Instant::now() >= attempt.deadline {
            if !all_complete && Instant::now() >= attempt.deadline {
                self.close_evidence
                    .record(RuntimeCloseCause::DeadlineExceeded);
            }
            self.close_state = OwnerCloseState::Incomplete {
                plugin_shutdown: attempt.plugin_shutdown,
            };
        } else {
            self.close_state = OwnerCloseState::Closing(attempt);
        }
    }

    pub(super) fn reporter(&self) -> &RuntimeFaultReporter {
        &self.runtime_fault_reporter
    }

    pub(crate) fn begin_fault_bridge_epoch(
        &mut self,
    ) -> Result<RuntimeFaultBridgeEpoch, RuntimeFault> {
        self.begin_fault_bridge_epoch_with_maintenance(|world| {
            world.increment_change_tick();
            world.check_change_ticks();
        })
    }

    fn begin_fault_bridge_epoch_with_maintenance(
        &mut self,
        maintenance: impl FnOnce(&mut World),
    ) -> Result<RuntimeFaultBridgeEpoch, RuntimeFault> {
        self.validate_fault_bridge_authority()?;
        let revision = self.world.resource::<RuntimeFaultBridgeRevision>().0;
        let reporter = self.runtime_fault_reporter.clone();
        let fault_before = reporter.fault();
        let fault_route = self.fault_route_token();
        with_runtime_system_fault_capture(fault_route, || {
            run_runtime_fault_bridge_maintenance(&mut self.world, maintenance)
        })??;
        self.validate_fault_bridge_authority()?;
        if self.world.resource::<RuntimeFaultBridgeRevision>().0 != revision {
            return Err(runtime_fault_bridge_authority_fault());
        }
        if fault_before.is_none()
            && let Some(fault) = reporter.fault()
        {
            return Err(fault);
        }
        let epoch = RuntimeFaultBridgeEpoch::capture(&self.world)
            .ok_or_else(runtime_fault_bridge_authority_fault)?;
        self.world.increment_change_tick();
        Ok(epoch)
    }

    pub(crate) fn validate_fault_bridge_epoch(
        &self,
        expected: RuntimeFaultBridgeEpoch,
    ) -> Result<(), RuntimeFault> {
        self.validate_fault_bridge_authority()?;
        if expected.is_unchanged(&self.world) {
            Ok(())
        } else {
            Err(runtime_fault_bridge_authority_fault())
        }
    }

    fn validate_fault_bridge_authority(&self) -> Result<(), RuntimeFault> {
        let Some(fault_route) = &self.fault_route else {
            return Err(runtime_fault_bridge_authority_fault());
        };
        if !fault_route.validate(&self.runtime_fault_reporter) {
            return Err(runtime_fault_bridge_authority_fault());
        }
        validate_runtime_fault_bridge_authority(
            &self.world,
            &self.runtime_fault_reporter,
            Some(fault_route.handler()),
            self.runtime_fault_bridge,
        )
    }

    pub(super) fn fault_route_token(&self) -> Option<FaultRouteToken> {
        self.fault_route.as_ref().map(FaultRouteLease::token)
    }

    pub(super) fn capture_driver_scope<R>(
        &mut self,
        authority_policy: DriverScopeAuthorityPolicy,
        operation: impl FnOnce(&mut World) -> R,
    ) -> Result<R, RuntimeFault> {
        let reporter = self.runtime_fault_reporter.clone();
        let fault_before = reporter.fault();
        if authority_policy == DriverScopeAuthorityPolicy::Full {
            self.validate_managed_runtime_authority()
                .map_err(|fault| reporter.record_canonical(fault))?;
        }
        let bridge_epoch = (authority_policy == DriverScopeAuthorityPolicy::Full)
            .then(|| self.begin_fault_bridge_epoch())
            .transpose()
            .map_err(|fault| reporter.record_canonical(fault))?;

        let fault_route = self.fault_route_token();
        let result =
            match with_runtime_system_fault_capture(fault_route, || operation(&mut self.world)) {
                Ok(result) => result,
                Err(fault) => return Err(reporter.record_canonical(fault)),
            };

        if let Some(bridge_epoch) = bridge_epoch
            && let Err(fault) = self.validate_fault_bridge_epoch(bridge_epoch)
        {
            return Err(reporter.record_canonical(fault));
        }
        if authority_policy == DriverScopeAuthorityPolicy::Full
            && let Err(fault) = self.validate_managed_runtime_authority()
        {
            return Err(reporter.record_canonical(fault));
        }
        if fault_before.is_none() {
            if let Some(fault) = reporter.fault() {
                return Err(fault);
            }
        }
        Ok(result)
    }

    pub(super) fn close_evidence(&self) -> &RuntimeCloseEvidence {
        &self.close_evidence
    }

    pub(super) fn close_state(&self) -> RuntimeCandidateRetirementState {
        match self.close_state {
            OwnerCloseState::Open | OwnerCloseState::Closing(_) => {
                RuntimeCandidateRetirementState::Retiring
            }
            OwnerCloseState::Incomplete { .. } => {
                RuntimeCandidateRetirementState::RetirementIncomplete
            }
            OwnerCloseState::Complete => RuntimeCandidateRetirementState::Retired,
        }
    }
}

impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        if matches!(state.close_state, OwnerCloseState::Complete) {
            if self.quarantine_accounted {
                record_reaped_runtime_owner();
            }
            drop(state);
            return;
        }
        if self.quarantine_accounted || std::thread::panicking() {
            retain_runtime_owner_state(state, self.quarantine_accounted);
            return;
        }

        let mut guard = RuntimeOwnerRetentionGuard::new(state);
        guard.finish_close_attempt();
        if matches!(guard.close_state, OwnerCloseState::Complete) {
            drop(guard.take_state());
        }
    }
}

struct RuntimeOwnerRetentionGuard {
    state: Option<RuntimeOwnerState>,
}

impl RuntimeOwnerRetentionGuard {
    fn new(state: RuntimeOwnerState) -> Self {
        Self { state: Some(state) }
    }

    fn take_state(&mut self) -> RuntimeOwnerState {
        self.state
            .take()
            .expect("runtime owner retention guard transfers state once")
    }
}

impl Deref for RuntimeOwnerRetentionGuard {
    type Target = RuntimeOwnerState;

    fn deref(&self) -> &Self::Target {
        self.state
            .as_ref()
            .expect("runtime owner retention guard remains armed while usable")
    }
}

impl DerefMut for RuntimeOwnerRetentionGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
            .as_mut()
            .expect("runtime owner retention guard remains armed while usable")
    }
}

impl Drop for RuntimeOwnerRetentionGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            retain_runtime_owner_state(state, false);
        }
    }
}

struct RuntimeOwnerQuarantine {
    owners: VecDeque<RuntimeOwnerState>,
    in_flight: usize,
}

impl RuntimeOwnerQuarantine {
    const fn new() -> Self {
        Self {
            owners: VecDeque::new(),
            in_flight: 0,
        }
    }

    fn occupancy(&self) -> usize {
        self.owners
            .len()
            .checked_add(self.in_flight)
            .unwrap_or_else(|| std::process::abort())
    }
}

impl Drop for RuntimeOwnerQuarantine {
    fn drop(&mut self) {
        if self.occupancy() != 0 {
            std::process::abort();
        }
    }
}

std::thread_local! {
    static RUNTIME_OWNER_QUARANTINE: RefCell<RuntimeOwnerQuarantine> =
        const { RefCell::new(RuntimeOwnerQuarantine::new()) };
}

fn runtime_quarantine_counts() -> (usize, usize) {
    let result = RUNTIME_OWNER_QUARANTINE.try_with(|quarantine| {
        quarantine
            .try_borrow()
            .map(|quarantine| (quarantine.owners.len(), quarantine.in_flight))
            .ok()
    });
    match result {
        Ok(Some(counts)) => counts,
        Ok(None) | Err(_) => std::process::abort(),
    }
}

fn runtime_quarantine_queued_len() -> usize {
    runtime_quarantine_counts().0
}

fn runtime_quarantine_occupancy() -> usize {
    let (queued, in_flight) = runtime_quarantine_counts();
    queued
        .checked_add(in_flight)
        .unwrap_or_else(|| std::process::abort())
}

fn pop_runtime_quarantine_owner() -> Option<RuntimeOwnerState> {
    let mut state = None;
    let result = RUNTIME_OWNER_QUARANTINE.try_with(|quarantine| {
        let Ok(mut quarantine) = quarantine.try_borrow_mut() else {
            return false;
        };
        state = quarantine.owners.pop_front();
        if state.is_some() {
            let Some(in_flight) = quarantine.in_flight.checked_add(1) else {
                return false;
            };
            quarantine.in_flight = in_flight;
        }
        true
    });
    if !matches!(result, Ok(true)) {
        std::process::abort();
    }
    state
}

fn retain_runtime_owner_state(state: RuntimeOwnerState, already_accounted: bool) {
    if already_accounted {
        retain_existing_runtime_owner(state);
    } else {
        retain_new_runtime_owner(state);
    }
}

fn retain_new_runtime_owner(state: RuntimeOwnerState) {
    let mut state = Some(state);
    let result = RUNTIME_OWNER_QUARANTINE.try_with(|quarantine| {
        let Ok(mut quarantine) = quarantine.try_borrow_mut() else {
            return false;
        };
        if quarantine.occupancy() >= MAX_QUARANTINED_RUNTIME_OWNERS_PER_THREAD
            || !reserve_process_quarantine_slot()
        {
            return false;
        }
        quarantine
            .owners
            .push_back(state.take().expect("quarantine state is inserted once"));
        true
    });
    if !matches!(result, Ok(true)) {
        std::process::abort();
    }
    increment_atomic_u64(&TOTAL_QUARANTINED_RUNTIME_OWNERS);
}

fn retain_existing_runtime_owner(state: RuntimeOwnerState) {
    let mut state = Some(state);
    let result = RUNTIME_OWNER_QUARANTINE.try_with(|quarantine| {
        let Ok(mut quarantine) = quarantine.try_borrow_mut() else {
            return false;
        };
        let Some(in_flight) = quarantine.in_flight.checked_sub(1) else {
            return false;
        };
        quarantine.in_flight = in_flight;
        quarantine
            .owners
            .push_back(state.take().expect("quarantine state is reinserted once"));
        quarantine.occupancy() <= MAX_QUARANTINED_RUNTIME_OWNERS_PER_THREAD
    });
    if !matches!(result, Ok(true)) {
        std::process::abort();
    }
}

fn record_reaped_runtime_owner() {
    let result = RUNTIME_OWNER_QUARANTINE.try_with(|quarantine| {
        let Ok(mut quarantine) = quarantine.try_borrow_mut() else {
            return false;
        };
        let Some(in_flight) = quarantine.in_flight.checked_sub(1) else {
            return false;
        };
        quarantine.in_flight = in_flight;
        true
    });
    if !matches!(result, Ok(true)) {
        std::process::abort();
    }
    if QUARANTINED_RUNTIME_OWNER_COUNT
        .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_sub(1)
        })
        .is_err()
    {
        std::process::abort();
    }
    increment_atomic_u64(&TOTAL_REAPED_RUNTIME_OWNERS);
}

fn reserve_process_quarantine_slot() -> bool {
    QUARANTINED_RUNTIME_OWNER_COUNT
        .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS).then_some(current + 1)
        })
        .is_ok()
}

fn increment_atomic_u64(counter: &AtomicU64) {
    if counter
        .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .is_err()
    {
        std::process::abort();
    }
}

fn deadline_after(timeout: Duration) -> Instant {
    Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now)
}
