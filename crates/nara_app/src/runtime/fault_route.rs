use std::sync::{Mutex, MutexGuard, OnceLock};

use bevy_ecs::error::{BevyError, ErrorContext, ErrorHandler, Severity, match_severity};

use super::{RuntimeExecutionError, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter};

pub(super) const RUNTIME_FAULT_ROUTE_CAPACITY: usize =
    super::MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS + 3;

static RUNTIME_FAULT_ROUTES: OnceLock<[FaultRouteSlot; RUNTIME_FAULT_ROUTE_CAPACITY]> =
    OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultRoutePhase {
    Vacant,
    Reserved,
    Active,
    Retiring,
    Quiescent,
}

#[derive(Debug)]
struct FaultRouteState {
    phase: FaultRoutePhase,
    epoch: u64,
    reporter: Option<RuntimeFaultReporter>,
    executor_in_flight: usize,
    handler_in_flight: usize,
}

impl FaultRouteState {
    const fn new() -> Self {
        Self {
            phase: FaultRoutePhase::Vacant,
            epoch: 0,
            reporter: None,
            executor_in_flight: 0,
            handler_in_flight: 0,
        }
    }

    fn reserve(&mut self) -> Option<u64> {
        if self.phase == FaultRoutePhase::Quiescent {
            debug_assert!(self.reporter.is_none());
            debug_assert_eq!(self.executor_in_flight, 0);
            debug_assert_eq!(self.handler_in_flight, 0);
            self.phase = FaultRoutePhase::Vacant;
        }
        if self.phase != FaultRoutePhase::Vacant {
            return None;
        }
        self.epoch = self.epoch.checked_add(1).unwrap_or_else(|| {
            std::process::abort();
        });
        self.phase = FaultRoutePhase::Reserved;
        Some(self.epoch)
    }

    fn activate(&mut self, epoch: u64, reporter: RuntimeFaultReporter) -> bool {
        if self.phase != FaultRoutePhase::Reserved || self.epoch != epoch {
            return false;
        }
        self.reporter = Some(reporter);
        self.phase = FaultRoutePhase::Active;
        true
    }

    fn release_reservation(&mut self, epoch: u64) {
        if self.phase != FaultRoutePhase::Reserved || self.epoch != epoch {
            return;
        }
        self.phase = FaultRoutePhase::Vacant;
    }

    fn begin_retirement(&mut self, epoch: u64) {
        if self.epoch != epoch {
            return;
        }
        if self.phase == FaultRoutePhase::Active {
            self.phase = FaultRoutePhase::Retiring;
        }
        self.finish_retirement_if_quiescent();
    }

    fn finish_retirement_if_quiescent(&mut self) {
        if self.phase != FaultRoutePhase::Retiring
            || self.executor_in_flight != 0
            || self.handler_in_flight != 0
        {
            return;
        }
        self.reporter = None;
        self.phase = FaultRoutePhase::Quiescent;
    }
}

struct FaultRouteSlot {
    state: Mutex<FaultRouteState>,
}

impl FaultRouteSlot {
    const fn new() -> Self {
        Self {
            state: Mutex::new(FaultRouteState::new()),
        }
    }
}

fn fault_route_slots() -> &'static [FaultRouteSlot; RUNTIME_FAULT_ROUTE_CAPACITY] {
    RUNTIME_FAULT_ROUTES.get_or_init(|| std::array::from_fn(|_| FaultRouteSlot::new()))
}

fn lock_slot(slot: usize) -> MutexGuard<'static, FaultRouteState> {
    fault_route_slots()[slot]
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug)]
pub(super) struct FaultRouteReservation {
    slot: usize,
    epoch: u64,
    armed: bool,
}

impl FaultRouteReservation {
    pub(super) fn try_acquire() -> Option<Self> {
        fault_route_slots()
            .iter()
            .enumerate()
            .find_map(|(slot, route)| {
                let mut state = route
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.reserve().map(|epoch| Self {
                    slot,
                    epoch,
                    armed: true,
                })
            })
    }

    pub(super) fn activate(mut self, reporter: RuntimeFaultReporter) -> FaultRouteLease {
        let activated = lock_slot(self.slot).activate(self.epoch, reporter);
        if !activated {
            std::process::abort();
        }
        self.armed = false;
        FaultRouteLease {
            slot: self.slot,
            epoch: self.epoch,
            armed: true,
        }
    }
}

impl Drop for FaultRouteReservation {
    fn drop(&mut self) {
        if self.armed {
            lock_slot(self.slot).release_reservation(self.epoch);
        }
    }
}

#[derive(Debug)]
pub(super) struct FaultRouteLease {
    slot: usize,
    epoch: u64,
    armed: bool,
}

impl FaultRouteLease {
    pub(super) fn handler(&self) -> ErrorHandler {
        RUNTIME_ERROR_HANDLERS[self.slot]
    }

    pub(super) const fn token(&self) -> FaultRouteToken {
        FaultRouteToken {
            slot: self.slot,
            epoch: self.epoch,
        }
    }

    pub(super) fn validate(&self, reporter: &RuntimeFaultReporter) -> bool {
        self.token().validate(reporter)
    }
}

impl Drop for FaultRouteLease {
    fn drop(&mut self) {
        if self.armed {
            lock_slot(self.slot).begin_retirement(self.epoch);
            self.armed = false;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FaultRouteToken {
    slot: usize,
    epoch: u64,
}

impl FaultRouteToken {
    pub(super) fn enter(self) -> Result<FaultRouteExecutionGuard, RuntimeFault> {
        let mut state = lock_slot(self.slot);
        if state.phase != FaultRoutePhase::Active || state.epoch != self.epoch {
            return Err(fault_route_authority_fault());
        }
        state.executor_in_flight = state
            .executor_in_flight
            .checked_add(1)
            .unwrap_or_else(|| std::process::abort());
        Ok(FaultRouteExecutionGuard {
            slot: self.slot,
            epoch: self.epoch,
            armed: true,
        })
    }

    fn validate(self, reporter: &RuntimeFaultReporter) -> bool {
        let state = lock_slot(self.slot);
        state.phase == FaultRoutePhase::Active
            && state.epoch == self.epoch
            && state
                .reporter
                .as_ref()
                .is_some_and(|candidate| candidate.has_authority(reporter))
    }
}

/// Prevents slot reuse from before Bevy can copy the handler until its executor scope returns.
///
/// The handler count alone cannot cover a callback that has started but has not acquired the slot
/// lock yet. Every managed schedule or exclusive World scope therefore holds this outer guard.
pub(super) struct FaultRouteExecutionGuard {
    slot: usize,
    epoch: u64,
    armed: bool,
}

impl Drop for FaultRouteExecutionGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut state = lock_slot(self.slot);
        if state.epoch == self.epoch {
            state.executor_in_flight = state
                .executor_in_flight
                .checked_sub(1)
                .unwrap_or_else(|| std::process::abort());
            state.finish_retirement_if_quiescent();
        }
        self.armed = false;
    }
}

struct FaultRouteHandlerGuard {
    slot: usize,
    epoch: u64,
    reporter: RuntimeFaultReporter,
}

impl Drop for FaultRouteHandlerGuard {
    fn drop(&mut self) {
        let mut state = lock_slot(self.slot);
        if state.epoch == self.epoch {
            state.handler_in_flight = state
                .handler_in_flight
                .checked_sub(1)
                .unwrap_or_else(|| std::process::abort());
            state.finish_retirement_if_quiescent();
        }
    }
}

fn enter_handler(slot: usize) -> Option<FaultRouteHandlerGuard> {
    let mut state = lock_slot(slot);
    if !matches!(
        state.phase,
        FaultRoutePhase::Active | FaultRoutePhase::Retiring
    ) {
        return None;
    }
    let reporter = state.reporter.clone()?;
    state.handler_in_flight = state
        .handler_in_flight
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
    Some(FaultRouteHandlerGuard {
        slot,
        epoch: state.epoch,
        reporter,
    })
}

fn fault_route_authority_fault() -> RuntimeFault {
    RuntimeFault::engine(
        RuntimeFaultKind::FaultReporterAuthority,
        "nara.app.runtime-fault-route",
    )
}

fn route_error<const SLOT: usize>(error: BevyError, context: ErrorContext) {
    let severity = error.severity();
    let Some(handler) = enter_handler(SLOT) else {
        match_severity(error, context);
        return;
    };
    if severity == Severity::Error {
        let kind = match &context {
            ErrorContext::System { .. } => RuntimeFaultKind::System,
            ErrorContext::RunCondition { .. } => RuntimeFaultKind::RunCondition,
            ErrorContext::Command { .. } => RuntimeFaultKind::Command,
            ErrorContext::Observer { .. } => RuntimeFaultKind::Observer,
        };
        let fault = error
            .downcast_ref::<RuntimeExecutionError>()
            .map(|classified| {
                RuntimeFault::engine_with_detail(
                    kind,
                    "nara.ecs.fallible-execution",
                    classified.detail(),
                )
            })
            .unwrap_or_else(|| RuntimeFault::engine(kind, "nara.ecs.fallible-execution"));
        handler.reporter.report(fault);
        return;
    }
    match_severity(error, context);
}

macro_rules! route_handlers {
    ($($slot:literal),+ $(,)?) => {
        [$(route_error::<$slot> as ErrorHandler),+]
    };
}

static RUNTIME_ERROR_HANDLERS: [ErrorHandler; RUNTIME_FAULT_ROUTE_CAPACITY] = route_handlers![
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,
    98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116,
    117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppRunError, RuntimeFaultDetail, RuntimeFaultDetailError};
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("Bearer private-token from C:\\Users\\private")]
    struct UnknownThirdPartyFault;

    fn command_context(name: &'static str) -> ErrorContext {
        ErrorContext::Command { name: name.into() }
    }

    #[test]
    fn capacity_covers_quarantine_overlap_and_one_pending_reservation() {
        assert_eq!(
            RUNTIME_FAULT_ROUTE_CAPACITY,
            super::super::MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS + 2 + 1
        );
        assert_eq!(RUNTIME_ERROR_HANDLERS.len(), RUNTIME_FAULT_ROUTE_CAPACITY);
    }

    #[test]
    fn stale_epoch_cannot_enter_or_release_a_reused_slot() {
        let mut state = FaultRouteState::new();
        let stale_epoch = state.reserve().expect("first reservation");
        assert!(state.activate(stale_epoch, RuntimeFaultReporter::new()));
        state.begin_retirement(stale_epoch);
        assert_eq!(state.phase, FaultRoutePhase::Quiescent);

        let replacement_epoch = state.reserve().expect("replacement reservation");
        assert_ne!(stale_epoch, replacement_epoch);
        assert!(state.activate(replacement_epoch, RuntimeFaultReporter::new()));

        state.begin_retirement(stale_epoch);
        assert_eq!(
            state.phase,
            FaultRoutePhase::Active,
            "a stale lease epoch must not retire a reused route"
        );
    }

    #[test]
    fn retirement_waits_for_executor_return() {
        let mut state = FaultRouteState::new();
        let epoch = state.reserve().expect("reservation");
        assert!(state.activate(epoch, RuntimeFaultReporter::new()));
        state.executor_in_flight = 1;

        state.begin_retirement(epoch);
        assert_eq!(state.phase, FaultRoutePhase::Retiring);

        state.executor_in_flight = 0;
        state.finish_retirement_if_quiescent();
        assert_eq!(state.phase, FaultRoutePhase::Quiescent);
    }

    #[test]
    fn retiring_route_keeps_its_reporter_until_old_handlers_return() {
        let reporter = RuntimeFaultReporter::new();
        let lease = FaultRouteReservation::try_acquire()
            .expect("one route is available")
            .activate(reporter.clone());
        let slot = lease.slot;
        let first_handler = enter_handler(slot).expect("active route accepts its handler");

        drop(lease);
        assert_eq!(lock_slot(slot).phase, FaultRoutePhase::Retiring);
        let late_handler = enter_handler(slot).expect("retiring route accepts an old callback");
        late_handler.reporter.report(RuntimeFault::engine(
            RuntimeFaultKind::System,
            "nara.test.retiring-fault-route",
        ));
        assert_eq!(reporter.fault().unwrap().kind(), RuntimeFaultKind::System);

        drop(late_handler);
        assert_eq!(lock_slot(slot).phase, FaultRoutePhase::Retiring);
        drop(first_handler);
        assert_eq!(lock_slot(slot).phase, FaultRoutePhase::Quiescent);
    }

    #[test]
    fn executor_scope_unwind_releases_its_in_flight_count() {
        let lease = FaultRouteReservation::try_acquire()
            .expect("one route is available")
            .activate(RuntimeFaultReporter::new());
        let token = lease.token();

        let unwind = std::panic::catch_unwind(|| {
            let _execution = token.enter().expect("active route accepts executor entry");
            panic!("injected executor unwind");
        });

        assert!(unwind.is_err());
        assert_eq!(lock_slot(token.slot).executor_in_flight, 0);
        drop(lease);
        assert_eq!(lock_slot(token.slot).phase, FaultRoutePhase::Quiescent);
    }

    #[test]
    fn runtime_fault_detail_rejects_unbounded_or_sensitive_metadata() {
        let maximum_code = Box::leak(
            "a".repeat(RuntimeFaultDetail::MAX_CODE_BYTES)
                .into_boxed_str(),
        );
        let oversized_code = Box::leak(
            "a".repeat(RuntimeFaultDetail::MAX_CODE_BYTES + 1)
                .into_boxed_str(),
        );
        let maximum_summary = Box::leak(
            "a".repeat(RuntimeFaultDetail::MAX_SUMMARY_BYTES)
                .into_boxed_str(),
        );
        let oversized_summary = Box::leak(
            "a".repeat(RuntimeFaultDetail::MAX_SUMMARY_BYTES + 1)
                .into_boxed_str(),
        );

        assert!(RuntimeFaultDetail::new(maximum_code, maximum_summary, "nara.test").is_ok());
        assert_eq!(
            RuntimeFaultDetail::new(oversized_code, "safe summary", "nara.test"),
            Err(RuntimeFaultDetailError::InvalidCode)
        );
        assert_eq!(
            RuntimeFaultDetail::new("nara.test", oversized_summary, "nara.test"),
            Err(RuntimeFaultDetailError::InvalidSummary)
        );
        for summary in [
            "Bearer test-token",
            "https://user:password@example.invalid/path",
            "C:\\Users\\private",
            "unsafe\u{202e}summary",
        ] {
            assert_eq!(
                RuntimeFaultDetail::new("nara.test", summary, "nara.test"),
                Err(RuntimeFaultDetailError::InvalidSummary)
            );
        }
        assert_eq!(
            RuntimeFaultDetail::new("nara.test", "safe summary", "secret-origin"),
            Err(RuntimeFaultDetailError::InvalidOrigin)
        );
    }

    #[test]
    fn classified_system_error_preserves_validated_detail_through_app_error_conversion() {
        let detail = RuntimeFaultDetail::new(
            "nara.test.startup-failed",
            "Startup initialization failed",
            "nara.test.startup",
        )
        .expect("test metadata is valid");
        let reporter = RuntimeFaultReporter::new();
        let lease = FaultRouteReservation::try_acquire()
            .expect("one route is available")
            .activate(reporter.clone());

        (lease.handler())(
            detail.into_bevy_error(),
            command_context("private::system::name"),
        );

        let fault = reporter.fault().expect("classified fault is retained");
        assert_eq!(fault.kind(), RuntimeFaultKind::Command);
        assert_eq!(fault.source(), "nara.ecs.fallible-execution");
        assert_eq!(fault.detail(), Some(detail));

        let app_error = AppRunError::managed_runtime_fault(&fault);
        assert_eq!(app_error.runtime_fault_detail(), Some(detail));
        let rebound = RuntimeFault::app(app_error);
        assert_eq!(rebound.kind(), RuntimeFaultKind::Command);
        assert_eq!(rebound.source(), "nara.ecs.fallible-execution");
        assert_eq!(rebound.detail(), Some(detail));
    }

    #[test]
    fn unknown_third_party_error_retains_no_dynamic_error_or_context_text() {
        let reporter = RuntimeFaultReporter::new();
        let lease = FaultRouteReservation::try_acquire()
            .expect("one route is available")
            .activate(reporter.clone());

        (lease.handler())(
            BevyError::error(UnknownThirdPartyFault),
            command_context("C:\\Users\\private\\system"),
        );

        let fault = reporter.fault().expect("generic fault is retained");
        assert_eq!(fault.kind(), RuntimeFaultKind::Command);
        assert_eq!(fault.source(), "nara.ecs.fallible-execution");
        assert_eq!(fault.detail(), None);
        let retained = format!("{fault:?}");
        assert!(!retained.contains("private-token"));
        assert!(!retained.contains("Users"));
    }
}
