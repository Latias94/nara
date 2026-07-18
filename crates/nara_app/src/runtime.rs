use std::{
    any::{TypeId, type_name},
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
    fmt::{self, Debug, Display, Formatter},
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, TryLockError,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use bevy_ecs::error::{
    BevyError, ErrorContext, ErrorHandler, FallbackErrorHandler, Severity, match_severity,
};
use nara_ecs::{
    Command, Mut, Resource, World, change_detection::Tick, component::Mutable,
    lifecycle::HookContext, world::DeferredWorld,
};
use thiserror::Error;

use crate::{
    App, AppFrameOutcome, AppRunError, PluginLifecycleState, PluginShutdownError,
    RuntimeTimeSettings, SealedApp,
};

const MAX_RETAINED_CONTROL_RESULTS: usize = 32;
const MAX_QUARANTINED_RUNTIME_OWNERS_PER_THREAD: usize = 32;
const MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS: usize = 128;
const DEFAULT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(1);
static QUARANTINED_RUNTIME_OWNER_COUNT: AtomicUsize = AtomicUsize::new(0);
static TOTAL_QUARANTINED_RUNTIME_OWNERS: AtomicU64 = AtomicU64::new(0);
static TOTAL_REAPED_RUNTIME_OWNERS: AtomicU64 = AtomicU64::new(0);
static RUNTIME_SCHEDULE_AUTHORITY: Mutex<()> = Mutex::new(());
static ACTIVE_RUNTIME_REPORTER: Mutex<Option<RuntimeFaultReporter>> = Mutex::new(None);

#[derive(Debug, Default, Resource)]
struct RuntimeFaultBridgeRevision(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeFaultBridgeEpoch {
    revision: u64,
    reporter_added: Tick,
    reporter_changed: Tick,
    handler_added: Tick,
    handler_changed: Tick,
}

impl RuntimeFaultBridgeEpoch {
    fn capture(world: &World) -> Option<Self> {
        let revision = world.get_resource::<RuntimeFaultBridgeRevision>()?.0;
        let reporter = world.get_resource_change_ticks::<RuntimeFaultReporter>()?;
        let handler = world.get_resource_change_ticks::<FallbackErrorHandler>()?;
        Some(Self {
            revision,
            reporter_added: reporter.added,
            reporter_changed: reporter.changed,
            handler_added: handler.added,
            handler_changed: handler.changed,
        })
    }
}

fn record_fault_bridge_structure_change(mut world: DeferredWorld<'_>, _context: HookContext) {
    let Some(mut revision) = world.get_resource_mut::<RuntimeFaultBridgeRevision>() else {
        return;
    };
    revision.0 = revision
        .0
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
}

pub(crate) fn initialize_runtime_fault_bridge(world: &mut World, reporter: RuntimeFaultReporter) {
    world
        .register_component_hooks::<RuntimeFaultReporter>()
        .on_insert(record_fault_bridge_structure_change)
        .on_discard(record_fault_bridge_structure_change);
    world
        .register_component_hooks::<FallbackErrorHandler>()
        .on_insert(record_fault_bridge_structure_change)
        .on_discard(record_fault_bridge_structure_change);
    world.insert_resource(RuntimeFaultBridgeRevision::default());
    world.insert_resource(reporter);
}

pub(crate) fn validate_managed_fault_boundary(
    world: &World,
    reporter: &RuntimeFaultReporter,
    generation: RuntimeGeneration,
) -> Result<(), AppRunError> {
    let fault = reporter
        .fault()
        .or_else(|| {
            (world.get_resource::<RuntimeGeneration>() != Some(&generation))
                .then(runtime_fault_bridge_authority_fault)
        })
        .or_else(|| validate_runtime_fault_bridge_authority(world, reporter).err());
    let Some(fault) = fault else {
        return Ok(());
    };
    let fault = reporter.record_canonical(fault);
    Err(AppRunError::managed_runtime(fault.kind(), fault.source()))
}

fn validate_runtime_fault_bridge_authority(
    world: &World,
    reporter: &RuntimeFaultReporter,
) -> Result<(), RuntimeFault> {
    let reporter_valid = world
        .get_resource::<RuntimeFaultReporter>()
        .is_some_and(|world_reporter| world_reporter.has_authority(reporter));
    let handler_valid = world
        .get_resource::<FallbackErrorHandler>()
        .is_some_and(|handler| {
            std::ptr::fn_addr_eq(handler.0, runtime_system_error_handler as ErrorHandler)
        });
    let revision_valid = world.contains_resource::<RuntimeFaultBridgeRevision>();
    if reporter_valid && handler_valid && revision_valid {
        Ok(())
    } else {
        Err(runtime_fault_bridge_authority_fault())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Resource)]
pub struct RuntimeGeneration(u64);

impl RuntimeGeneration {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl Debug for RuntimeGeneration {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RuntimeGeneration")
            .field(&self.0)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Running,
    Paused,
    Stepping,
    Faulted,
    Stopping,
    CloseIncomplete,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeFrameMode {
    Running,
    Paused,
}

impl RuntimeFrameMode {
    const fn is_paused(self) -> bool {
        matches!(self, Self::Paused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFaultKind {
    AppFrame,
    FaultReporterAuthority,
    ScheduleAuthority,
    System,
    RunCondition,
    Command,
    Observer,
    GameplayLifecycle,
    LocalIntentLoss,
    RequiredTask,
    RequiredService,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeFault {
    kind: RuntimeFaultKind,
    source: &'static str,
    app_error: Option<AppRunError>,
}

impl RuntimeFault {
    #[must_use]
    pub const fn engine(kind: RuntimeFaultKind, source: &'static str) -> Self {
        Self {
            kind,
            source,
            app_error: None,
        }
    }

    #[must_use]
    pub(crate) fn app(error: AppRunError) -> Self {
        Self {
            kind: RuntimeFaultKind::AppFrame,
            source: "nara.app.frame",
            app_error: Some(error),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> RuntimeFaultKind {
        self.kind
    }

    #[must_use]
    pub const fn source(&self) -> &'static str {
        self.source
    }

    #[must_use]
    pub const fn app_error(&self) -> Option<&AppRunError> {
        self.app_error.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimePublicationPhase {
    Candidate,
    Published,
    Closing,
    Closed,
}

#[derive(Debug)]
struct RuntimeFaultState {
    publication: RuntimePublicationPhase,
}

#[derive(Debug)]
struct RuntimeFaultCell {
    state: Mutex<RuntimeFaultState>,
    first_fault: OnceLock<RuntimeFault>,
}

#[derive(Clone, Resource)]
pub struct RuntimeFaultReporter {
    cell: Arc<RuntimeFaultCell>,
}

impl Debug for RuntimeFaultReporter {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeFaultReporter")
            .field("fault", &self.fault_ref())
            .finish_non_exhaustive()
    }
}

impl Default for RuntimeFaultReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeFaultReporter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cell: Arc::new(RuntimeFaultCell {
                state: Mutex::new(RuntimeFaultState {
                    publication: RuntimePublicationPhase::Candidate,
                }),
                first_fault: OnceLock::new(),
            }),
        }
    }

    fn has_authority(&self, reporter: &Self) -> bool {
        Arc::ptr_eq(&self.cell, &reporter.cell)
    }

    /// Records the first fault and returns whether this call won that race.
    pub fn report(&self, fault: RuntimeFault) -> bool {
        let _state = lock_unpoisoned(&self.cell.state);
        if self.cell.first_fault.get().is_some() {
            return false;
        }
        self.cell.first_fault.set(fault).is_ok()
    }

    #[must_use]
    pub fn fault(&self) -> Option<RuntimeFault> {
        self.fault_ref().cloned()
    }

    fn fault_ref(&self) -> Option<&RuntimeFault> {
        self.cell.first_fault.get()
    }

    fn record_canonical(&self, fault: RuntimeFault) -> RuntimeFault {
        let _state = lock_unpoisoned(&self.cell.state);
        if self.cell.first_fault.get().is_none() {
            let _ = self.cell.first_fault.set(fault);
        }
        self.cell
            .first_fault
            .get()
            .cloned()
            .expect("recording a runtime fault initializes the first-fault cell")
    }

    fn publish(&self) -> Option<RuntimeFault> {
        let mut state = lock_unpoisoned(&self.cell.state);
        debug_assert_eq!(state.publication, RuntimePublicationPhase::Candidate);
        let fault = self.cell.first_fault.get().cloned();
        state.publication = RuntimePublicationPhase::Published;
        fault
    }

    fn mark_closing(&self) {
        let mut state = lock_unpoisoned(&self.cell.state);
        if state.publication != RuntimePublicationPhase::Closed {
            state.publication = RuntimePublicationPhase::Closing;
        }
    }

    fn mark_closed(&self) {
        lock_unpoisoned(&self.cell.state).publication = RuntimePublicationPhase::Closed;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeCloseParticipantId(&'static str);

impl RuntimeCloseParticipantId {
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Debug for RuntimeCloseParticipantId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RuntimeCloseParticipantId")
            .field(&self.0)
            .finish()
    }
}

impl Display for RuntimeCloseParticipantId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseProgress {
    Pending,
    Complete,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("runtime close participant failed with code {code}")]
pub struct RuntimeCloseParticipantError {
    code: &'static str,
    disposition: RuntimeCloseErrorDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseErrorDisposition {
    Retryable,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseParticipantPhase {
    Begin,
    Poll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseCause {
    PluginShutdown,
    ParticipantError {
        participant: RuntimeCloseParticipantId,
        phase: RuntimeCloseParticipantPhase,
        code: &'static str,
        disposition: RuntimeCloseErrorDisposition,
    },
    DeadlineExceeded,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeCloseEvidence {
    causes: Vec<RuntimeCloseCause>,
}

impl RuntimeCloseEvidence {
    #[must_use]
    pub fn causes(&self) -> &[RuntimeCloseCause] {
        &self.causes
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.causes.is_empty()
    }

    #[must_use]
    pub fn plugin_shutdown_failed(&self) -> bool {
        self.causes.contains(&RuntimeCloseCause::PluginShutdown)
    }

    fn record(&mut self, cause: RuntimeCloseCause) {
        if !self.causes.contains(&cause) {
            self.causes.push(cause);
        }
    }
}

impl RuntimeCloseParticipantError {
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self::retryable(code)
    }

    #[must_use]
    pub const fn retryable(code: &'static str) -> Self {
        Self {
            code,
            disposition: RuntimeCloseErrorDisposition::Retryable,
        }
    }

    #[must_use]
    pub const fn terminal(code: &'static str) -> Self {
        Self {
            code,
            disposition: RuntimeCloseErrorDisposition::Terminal,
        }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn disposition(self) -> RuntimeCloseErrorDisposition {
        self.disposition
    }
}

pub struct RuntimeCloseContext<'world> {
    world: &'world mut World,
}

pub struct RuntimeDriverScope<'world> {
    world: &'world mut World,
}

/// Short-lived command application while a runtime candidate is still unpublished.
///
/// This scope exists for product Hosts that must materialize validated project state before
/// startup. The candidate revalidates its sticky fault bridge after the scope returns, so replacing
/// protected runtime authority cannot silently publish a healthy runtime.
pub struct RuntimeCandidateScope<'world> {
    world: &'world mut World,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWorldAccessError {
    #[error("runtime-managed ECS type {type_name} is protected from scoped mutation")]
    ProtectedType { type_name: &'static str },
    #[error("runtime scoped resource {type_name} is missing")]
    MissingResource { type_name: &'static str },
}

fn ensure_scoped_type_is_unprotected<T: 'static>() -> Result<(), RuntimeWorldAccessError> {
    let type_id = TypeId::of::<T>();
    if type_id == TypeId::of::<RuntimeFaultReporter>()
        || type_id == TypeId::of::<FallbackErrorHandler>()
        || type_id == TypeId::of::<RuntimeFaultBridgeRevision>()
    {
        Err(RuntimeWorldAccessError::ProtectedType {
            type_name: type_name::<T>(),
        })
    } else {
        Ok(())
    }
}

impl RuntimeDriverScope<'_> {
    #[must_use]
    pub fn world(&self) -> &World {
        self.world
    }

    pub fn get_resource_mut<R: Resource<Mutability = Mutable>>(
        &mut self,
    ) -> Result<Option<Mut<'_, R>>, RuntimeWorldAccessError> {
        ensure_scoped_type_is_unprotected::<R>()?;
        Ok(self.world.get_resource_mut::<R>())
    }

    pub fn resource_mut<R: Resource<Mutability = Mutable>>(
        &mut self,
    ) -> Result<Mut<'_, R>, RuntimeWorldAccessError> {
        self.get_resource_mut::<R>()?
            .ok_or(RuntimeWorldAccessError::MissingResource {
                type_name: type_name::<R>(),
            })
    }

    pub fn get_non_send_resource_mut<R: 'static>(
        &mut self,
    ) -> Result<Option<Mut<'_, R>>, RuntimeWorldAccessError> {
        ensure_scoped_type_is_unprotected::<R>()?;
        Ok(self.world.get_non_send_mut::<R>())
    }
}

impl RuntimeCandidateScope<'_> {
    /// Applies one owned ECS command immediately inside the exclusive admission transaction.
    ///
    /// The command cannot retain the candidate `World`, and protected runtime authority is
    /// revalidated after the enclosing admission scope returns.
    pub fn apply_command(&mut self, command: impl Command<Out = ()>) {
        command.apply(self.world);
    }
}

/// Failure returned by a short-lived managed runtime scope.
///
/// Healthy scopes verify the canonical fault reporter and fallback error handler around the
/// operation. Runtime-managed fault resources are unavailable through scoped mutable access.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeScopeError {
    #[error("runtime world access is unavailable while runtime state is {state:?}")]
    Unavailable { state: RuntimeState },
    #[error("runtime world access observed a runtime fault")]
    Faulted { fault: RuntimeFault },
}

impl RuntimeScopeError {
    #[must_use]
    pub const fn fault(&self) -> Option<&RuntimeFault> {
        match self {
            Self::Unavailable { .. } => None,
            Self::Faulted { fault } => Some(fault),
        }
    }
}

impl RuntimeCloseContext<'_> {
    #[must_use]
    pub fn world(&self) -> &World {
        self.world
    }

    pub fn get_resource_mut<R: Resource<Mutability = Mutable>>(
        &mut self,
    ) -> Result<Option<Mut<'_, R>>, RuntimeWorldAccessError> {
        ensure_scoped_type_is_unprotected::<R>()?;
        Ok(self.world.get_resource_mut::<R>())
    }

    pub fn resource_mut<R: Resource<Mutability = Mutable>>(
        &mut self,
    ) -> Result<Mut<'_, R>, RuntimeWorldAccessError> {
        self.get_resource_mut::<R>()?
            .ok_or(RuntimeWorldAccessError::MissingResource {
                type_name: type_name::<R>(),
            })
    }

    pub fn insert_resource<R: Resource>(
        &mut self,
        resource: R,
    ) -> Result<(), RuntimeWorldAccessError> {
        ensure_scoped_type_is_unprotected::<R>()?;
        self.world.insert_resource(resource);
        Ok(())
    }

    pub fn remove_resource<R: Resource>(&mut self) -> Result<Option<R>, RuntimeWorldAccessError> {
        ensure_scoped_type_is_unprotected::<R>()?;
        Ok(self.world.remove_resource::<R>())
    }
}

pub trait RuntimeCloseParticipant: Send + 'static {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError>;

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError>;
}

struct RuntimeCloseEntry {
    id: RuntimeCloseParticipantId,
    participant: Box<dyn RuntimeCloseParticipant>,
    begun: bool,
    complete: bool,
    terminal_failure: bool,
}

pub struct RuntimeObligationLedger {
    ids: BTreeSet<RuntimeCloseParticipantId>,
    entries: Vec<RuntimeCloseEntry>,
}

impl Default for RuntimeObligationLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for RuntimeObligationLedger {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeObligationLedger")
            .field("ids", &self.ids)
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl RuntimeObligationLedger {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ids: BTreeSet::new(),
            entries: Vec::new(),
        }
    }

    pub fn register<P>(
        &mut self,
        id: RuntimeCloseParticipantId,
        participant: P,
    ) -> Result<(), RuntimeObligationRegistrationError<P>>
    where
        P: RuntimeCloseParticipant,
    {
        if !self.ids.insert(id) {
            return Err(RuntimeObligationRegistrationError {
                error: RuntimeObligationLedgerError::Duplicate { id },
                participant,
            });
        }
        self.entries.push(RuntimeCloseEntry {
            id,
            participant: Box::new(participant),
            begun: false,
            complete: false,
            terminal_failure: false,
        });
        Ok(())
    }

    pub(crate) fn append(&mut self, mut other: Self) -> Result<(), RuntimeObligationLedgerError> {
        self.preflight_append(&other)?;
        self.ids.append(&mut other.ids);
        self.entries.append(&mut other.entries);
        Ok(())
    }

    fn preflight_append(&self, other: &Self) -> Result<(), RuntimeObligationLedgerError> {
        if let Some(id) = self.ids.intersection(&other.ids).next().copied() {
            return Err(RuntimeObligationLedgerError::Duplicate { id });
        }
        Ok(())
    }

    fn append_for_retirement(&mut self, mut other: Self) {
        self.ids.append(&mut other.ids);
        self.entries.append(&mut other.entries);
    }

    pub(crate) fn retain_for_retirement<P>(&mut self, id: RuntimeCloseParticipantId, participant: P)
    where
        P: RuntimeCloseParticipant,
    {
        self.entries.push(RuntimeCloseEntry {
            id,
            participant: Box::new(participant),
            begun: false,
            complete: false,
            terminal_failure: false,
        });
    }

    pub(crate) fn drive_close_once(&mut self, world: &mut World) -> bool {
        let mut evidence = RuntimeCloseEvidence::default();
        self.drive_close_once_with_evidence(world, &mut evidence)
    }

    fn drive_close_once_with_evidence(
        &mut self,
        world: &mut World,
        evidence: &mut RuntimeCloseEvidence,
    ) -> bool {
        let mut failed = false;
        for entry in self.entries.iter_mut().rev() {
            if entry.complete || entry.terminal_failure {
                continue;
            }
            let mut context = RuntimeCloseContext { world };
            if !entry.begun {
                entry.begun = true;
                match entry.participant.begin_close(&mut context) {
                    Ok(RuntimeCloseProgress::Complete) => {
                        entry.complete = true;
                        continue;
                    }
                    Ok(RuntimeCloseProgress::Pending) => {}
                    Err(error) => {
                        evidence.record(RuntimeCloseCause::ParticipantError {
                            participant: entry.id,
                            phase: RuntimeCloseParticipantPhase::Begin,
                            code: error.code(),
                            disposition: error.disposition(),
                        });
                        entry.terminal_failure =
                            error.disposition() == RuntimeCloseErrorDisposition::Terminal;
                        if !entry.terminal_failure {
                            entry.begun = false;
                        }
                        failed = true;
                        continue;
                    }
                }
            }
            match entry.participant.poll_close(&mut context) {
                Ok(RuntimeCloseProgress::Complete) => {
                    entry.complete = true;
                }
                Ok(RuntimeCloseProgress::Pending) => {}
                Err(error) => {
                    evidence.record(RuntimeCloseCause::ParticipantError {
                        participant: entry.id,
                        phase: RuntimeCloseParticipantPhase::Poll,
                        code: error.code(),
                        disposition: error.disposition(),
                    });
                    entry.terminal_failure =
                        error.disposition() == RuntimeCloseErrorDisposition::Terminal;
                    failed = true;
                }
            }
        }
        failed
    }

    fn is_close_complete(&self) -> bool {
        self.entries.iter().all(|entry| entry.complete)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeObligationLedgerError {
    #[error("runtime close participant {id} is registered more than once")]
    Duplicate { id: RuntimeCloseParticipantId },
}

pub struct RuntimeObligationRegistrationError<P> {
    error: RuntimeObligationLedgerError,
    participant: P,
}

impl<P> Debug for RuntimeObligationRegistrationError<P> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeObligationRegistrationError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<P> Display for RuntimeObligationRegistrationError<P> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.error, formatter)
    }
}

impl<P> std::error::Error for RuntimeObligationRegistrationError<P> where P: 'static {}

impl<P> RuntimeObligationRegistrationError<P> {
    #[must_use]
    pub const fn error(&self) -> RuntimeObligationLedgerError {
        self.error
    }

    #[must_use]
    pub fn into_parts(self) -> (RuntimeObligationLedgerError, P) {
        (self.error, self.participant)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeClosePolicy {
    timeout: Duration,
}

impl Default for RuntimeClosePolicy {
    fn default() -> Self {
        Self::new(DEFAULT_CLOSE_TIMEOUT)
    }
}

impl RuntimeClosePolicy {
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }
}

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

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAdmissionError {
    #[error("a managed runtime requires an unstarted app")]
    AppStarted,
    #[error("a managed runtime cannot admit an app with a raw runner")]
    RawRunnerInstalled,
    #[error("a managed runtime requires a sealed ready app, got {state:?}")]
    AppNotReady { state: PluginLifecycleState },
    #[error("runtime generations are exhausted")]
    GenerationExhausted,
    #[error(transparent)]
    Obligation(#[from] RuntimeObligationLedgerError),
}

pub struct RuntimeAdmissionFailure {
    error: RuntimeAdmissionError,
    sealed: Option<Box<SealedApp>>,
    obligations: Option<RuntimeObligationLedger>,
    close_policy: RuntimeClosePolicy,
}

impl Debug for RuntimeAdmissionFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeAdmissionFailure")
            .field("error", &self.error)
            .field("obligations", &self.obligations)
            .finish_non_exhaustive()
    }
}

impl Display for RuntimeAdmissionFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for RuntimeAdmissionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl RuntimeAdmissionFailure {
    fn new(
        error: RuntimeAdmissionError,
        sealed: SealedApp,
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Self {
        Self {
            error,
            sealed: Some(Box::new(sealed)),
            obligations: Some(obligations),
            close_policy,
        }
    }

    #[must_use]
    pub const fn error(&self) -> RuntimeAdmissionError {
        self.error
    }

    #[must_use]
    pub fn begin_retirement(mut self) -> RuntimeAdmissionRetirement {
        let error = self.error;
        let mut owner = self.take_owner();
        owner.begin_close();
        RuntimeAdmissionRetirement { error, owner }
    }

    #[must_use]
    pub fn into_inputs(mut self) -> (SealedApp, RuntimeObligationLedger, RuntimeClosePolicy) {
        let sealed = *self
            .sealed
            .take()
            .expect("admission failure retains its sealed app until consumed");
        let obligations = self
            .obligations
            .take()
            .expect("admission failure retains its obligation ledger until consumed");
        (sealed, obligations, self.close_policy)
    }

    fn take_owner(&mut self) -> RuntimeOwner {
        let sealed = self
            .sealed
            .take()
            .expect("admission failure transfers its sealed app once");
        let mut obligations = self
            .obligations
            .take()
            .expect("admission failure transfers its obligation ledger once");
        let mut app = sealed.app;
        let reporter = app.runtime_fault_reporter.clone();
        obligations.append_for_retirement(app.take_runtime_obligations());
        RuntimeOwner::new(app, obligations, reporter, self.close_policy)
    }
}

impl Drop for RuntimeAdmissionFailure {
    fn drop(&mut self) {
        if self.sealed.is_some() {
            drop(self.take_owner());
        }
    }
}

pub struct RuntimeAdmissionRetirement {
    error: RuntimeAdmissionError,
    owner: RuntimeOwner,
}

impl Debug for RuntimeAdmissionRetirement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeAdmissionRetirement")
            .field("error", &self.error)
            .field("state", &self.retirement_state())
            .finish_non_exhaustive()
    }
}

impl Display for RuntimeAdmissionRetirement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "runtime admission failed: {}; retirement={:?}",
            self.error,
            self.retirement_state()
        )
    }
}

impl std::error::Error for RuntimeAdmissionRetirement {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl RuntimeAdmissionRetirement {
    #[must_use]
    pub const fn error(&self) -> RuntimeAdmissionError {
        self.error
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.owner.close_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.owner.close_evidence()
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        if self.owner.close_state() == RuntimeCandidateRetirementState::RetirementIncomplete {
            self.owner.retry_close();
        }
        self.owner.drive_close();
        self.retirement_state()
    }
}

pub struct RuntimeCandidate {
    owner: RuntimeOwner,
    generation: RuntimeGeneration,
}

impl Debug for RuntimeCandidate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeCandidate")
            .field("generation", &self.generation)
            .field("started", &self.owner.app.started())
            .finish_non_exhaustive()
    }
}

impl RuntimeCandidate {
    pub fn admit(sealed: SealedApp) -> Result<Self, RuntimeAdmissionFailure> {
        Self::admit_with(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
    }

    pub fn admit_with(
        sealed: SealedApp,
        mut obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Result<Self, RuntimeAdmissionFailure> {
        if sealed.app.started() {
            return Err(RuntimeAdmissionFailure::new(
                RuntimeAdmissionError::AppStarted,
                sealed,
                obligations,
                close_policy,
            ));
        }
        if sealed.app.has_raw_runner() {
            return Err(RuntimeAdmissionFailure::new(
                RuntimeAdmissionError::RawRunnerInstalled,
                sealed,
                obligations,
                close_policy,
            ));
        }
        if sealed.app.plugin_lifecycle_state() != PluginLifecycleState::Ready {
            return Err(RuntimeAdmissionFailure::new(
                RuntimeAdmissionError::AppNotReady {
                    state: sealed.app.plugin_lifecycle_state(),
                },
                sealed,
                obligations,
                close_policy,
            ));
        }
        if let Err(error) = obligations.preflight_append(&sealed.app.runtime_obligations) {
            return Err(RuntimeAdmissionFailure::new(
                error.into(),
                sealed,
                obligations,
                close_policy,
            ));
        }

        let generation = match allocate_generation() {
            Ok(generation) => generation,
            Err(error) => {
                return Err(RuntimeAdmissionFailure::new(
                    error,
                    sealed,
                    obligations,
                    close_policy,
                ));
            }
        };
        let mut app = sealed.app;
        obligations
            .append(app.take_runtime_obligations())
            .expect("runtime obligation conflicts were preflighted");
        let reporter = app.runtime_fault_reporter.clone();
        app.managed_runtime_generation = Some(generation);
        app.world.insert_resource(generation);
        app.world
            .insert_resource(FallbackErrorHandler(runtime_system_error_handler));

        Ok(Self {
            owner: RuntimeOwner::new(app, obligations, reporter, close_policy),
            generation,
        })
    }

    #[must_use]
    pub const fn generation(&self) -> RuntimeGeneration {
        self.generation
    }

    #[must_use]
    pub fn world(&self) -> &World {
        self.owner.app.world()
    }

    #[must_use]
    pub fn fault_reporter(&self) -> RuntimeFaultReporter {
        self.owner.reporter().clone()
    }

    /// Runs one exclusive Host admission transaction before startup and publication.
    ///
    /// The mutable World reference cannot escape the closure. Runtime fault authority is checked
    /// before and after the operation, and any bridge mutation becomes a sticky candidate fault.
    pub fn with_admission_scope<R>(
        &mut self,
        operation: impl FnOnce(&mut RuntimeCandidateScope<'_>) -> R,
    ) -> Result<R, RuntimeScopeError> {
        self.owner
            .capture_driver_scope(|world| {
                let mut scope = RuntimeCandidateScope { world };
                operation(&mut scope)
            })
            .map_err(|fault| RuntimeScopeError::Faulted { fault })
    }

    pub fn begin_retirement(mut self) -> RuntimeRetirement {
        self.owner.begin_close();
        RuntimeRetirement::new(self.owner, self.generation)
    }

    pub fn complete_startup(mut self) -> Result<ReadyRuntimeCandidate, RuntimeCandidateFailure> {
        let reporter = self.owner.reporter().clone();
        let bridge_epoch = match self.owner.begin_fault_bridge_epoch() {
            Ok(epoch) => epoch,
            Err(fault) => {
                return Err(RuntimeCandidateFailure::new(
                    self.owner,
                    self.generation,
                    fault,
                ));
            }
        };
        if let Some(fault) = reporter.fault() {
            return Err(RuntimeCandidateFailure::new(
                self.owner,
                self.generation,
                fault,
            ));
        }
        let startup =
            with_runtime_system_fault_capture(&reporter, || self.owner.app.complete_startup_once());
        let fault = match startup {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(RuntimeFault::app(error)),
            Err(fault) => Some(fault),
        }
        .or_else(|| reporter.fault())
        .or_else(|| {
            self.owner
                .app
                .prepare_managed_runtime()
                .err()
                .map(RuntimeFault::app)
        })
        .or_else(|| self.owner.validate_fault_bridge_epoch(bridge_epoch).err());

        if let Some(fault) = fault {
            return Err(RuntimeCandidateFailure::new(
                self.owner,
                self.generation,
                fault,
            ));
        }

        Ok(ReadyRuntimeCandidate {
            owner: self.owner,
            generation: self.generation,
        })
    }
}

/// Retryable owner retained when a repeatable plugin plan cannot be instantiated.
///
/// Retained-only preparation may fail before an `App` exists and therefore start complete.
/// Managed runtime construction creates the `App` with the Host ledger before preparation, so its
/// preparation, build, and finish failures retain that `App` and every registered runtime close
/// participant until cleanup completes or ownership moves to process quarantine during unwinding.
#[must_use = "failed runtime preparation retains close authority until cleanup completes"]
pub(crate) struct RuntimePreparationRetirement {
    owner: Option<RuntimeOwner>,
}

impl Debug for RuntimePreparationRetirement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePreparationRetirement")
            .field("state", &self.retirement_state())
            .finish_non_exhaustive()
    }
}

impl RuntimePreparationRetirement {
    pub(crate) fn complete() -> Self {
        Self { owner: None }
    }

    pub(crate) fn from_app(app: App, close_policy: RuntimeClosePolicy) -> Self {
        Self::from_app_with_obligations(app, RuntimeObligationLedger::new(), close_policy)
    }

    pub(crate) fn from_app_with_obligations(
        mut app: App,
        mut obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Self {
        obligations.append_for_retirement(app.take_runtime_obligations());
        let reporter = app.runtime_fault_reporter.clone();
        let mut owner = RuntimeOwner::new(app, obligations, reporter, close_policy);
        owner.begin_close();
        Self { owner: Some(owner) }
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.owner
            .as_ref()
            .map_or(RuntimeCandidateRetirementState::Retired, |owner| {
                owner.close_state()
            })
    }

    #[must_use]
    pub fn close_evidence(&self) -> Option<&RuntimeCloseEvidence> {
        self.owner.as_ref().map(|owner| owner.close_evidence())
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        let Some(owner) = &mut self.owner else {
            return RuntimeCandidateRetirementState::Retired;
        };
        if owner.close_state() == RuntimeCandidateRetirementState::RetirementIncomplete {
            owner.retry_close();
        }
        owner.drive_close();
        owner.close_state()
    }
}

pub struct ReadyRuntimeCandidate {
    owner: RuntimeOwner,
    generation: RuntimeGeneration,
}

/// Host-owned destination for one atomically published runtime owner.
///
/// The contained runtime cannot be replaced through this API. A product Host reserves the slot,
/// publishes one ready candidate into it, then drives or retires that same owner.
#[must_use = "runtime publication slots retain the published runtime owner"]
pub struct RuntimePublicationSlot {
    state: RuntimePublicationSlotState,
}

enum RuntimePublicationSlotState {
    Vacant,
    Published(Box<RuntimeInstance>),
    Consumed,
}

impl Debug for RuntimePublicationSlot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePublicationSlot")
            .field(
                "state",
                &match &self.state {
                    RuntimePublicationSlotState::Vacant => "vacant",
                    RuntimePublicationSlotState::Published(_) => "published",
                    RuntimePublicationSlotState::Consumed => "consumed",
                },
            )
            .finish_non_exhaustive()
    }
}

impl Default for RuntimePublicationSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimePublicationSlot {
    pub const fn new() -> Self {
        Self {
            state: RuntimePublicationSlotState::Vacant,
        }
    }

    #[must_use]
    pub const fn is_vacant(&self) -> bool {
        matches!(self.state, RuntimePublicationSlotState::Vacant)
    }

    #[must_use]
    pub const fn runtime(&self) -> Option<&RuntimeInstance> {
        match &self.state {
            RuntimePublicationSlotState::Published(runtime) => Some(runtime),
            RuntimePublicationSlotState::Vacant | RuntimePublicationSlotState::Consumed => None,
        }
    }

    #[must_use]
    pub const fn runtime_mut(&mut self) -> Option<&mut RuntimeInstance> {
        match &mut self.state {
            RuntimePublicationSlotState::Published(runtime) => Some(runtime),
            RuntimePublicationSlotState::Vacant | RuntimePublicationSlotState::Consumed => None,
        }
    }

    #[must_use]
    pub fn take(&mut self) -> Option<RuntimeInstance> {
        let state = std::mem::replace(&mut self.state, RuntimePublicationSlotState::Consumed);
        match state {
            RuntimePublicationSlotState::Published(runtime) => Some(*runtime),
            RuntimePublicationSlotState::Vacant => {
                self.state = RuntimePublicationSlotState::Vacant;
                None
            }
            RuntimePublicationSlotState::Consumed => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimePublicationError {
    DestinationOccupied,
    CandidateFault(RuntimeFault),
    InvalidCandidatePhase,
}

/// Retryable owner retained when atomic publication rejects a ready candidate.
#[must_use = "failed runtime publication retains close authority until cleanup completes"]
pub struct RuntimePublicationFailure {
    error: RuntimePublicationError,
    retirement: RuntimeRetirement,
}

impl RuntimePublicationFailure {
    fn rejected(mut candidate: ReadyRuntimeCandidate, error: RuntimePublicationError) -> Self {
        candidate.owner.begin_close();
        Self {
            error,
            retirement: RuntimeRetirement::new(candidate.owner, candidate.generation),
        }
    }

    fn faulted(
        mut owner: RuntimeOwner,
        generation: RuntimeGeneration,
        fault: RuntimeFault,
    ) -> Self {
        owner.reporter().record_canonical(fault.clone());
        owner.begin_close();
        Self {
            error: RuntimePublicationError::CandidateFault(fault),
            retirement: RuntimeRetirement::new(owner, generation),
        }
    }

    #[must_use]
    pub const fn error(&self) -> &RuntimePublicationError {
        &self.error
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.retirement.close_evidence()
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        self.retirement.drive_retirement()
    }
}

impl Debug for RuntimePublicationFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePublicationFailure")
            .field("error", &self.error)
            .field("retirement", &self.retirement_state())
            .finish_non_exhaustive()
    }
}

impl Debug for ReadyRuntimeCandidate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadyRuntimeCandidate")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl ReadyRuntimeCandidate {
    pub fn begin_retirement(mut self) -> RuntimeRetirement {
        self.owner.begin_close();
        RuntimeRetirement::new(self.owner, self.generation)
    }

    #[must_use]
    pub fn promote(self) -> RuntimeInstance {
        let fault = self.owner.reporter().publish();
        RuntimeInstance::from_promotion(self.owner, self.generation, fault)
    }

    /// Publishes this candidate directly into an empty Host-owned runtime slot.
    ///
    /// The fault reporter is locked across the final candidate check, owner transfer, and
    /// publication marker. A concurrent fault therefore either rejects the candidate before the
    /// transfer or observes a runtime that is already owned by the destination slot.
    ///
    pub fn publish_into(
        self,
        destination: &mut RuntimePublicationSlot,
    ) -> Result<(), RuntimePublicationFailure> {
        if !destination.is_vacant() {
            return Err(RuntimePublicationFailure::rejected(
                self,
                RuntimePublicationError::DestinationOccupied,
            ));
        }

        let Self { owner, generation } = self;
        let reporter = owner.reporter().clone();
        let mut state = lock_unpoisoned(&reporter.cell.state);
        if state.publication != RuntimePublicationPhase::Candidate {
            drop(state);
            return Err(RuntimePublicationFailure::rejected(
                Self { owner, generation },
                RuntimePublicationError::InvalidCandidatePhase,
            ));
        }
        if let Some(fault) = reporter.cell.first_fault.get().cloned() {
            drop(state);
            return Err(RuntimePublicationFailure::faulted(owner, generation, fault));
        }

        destination.state = RuntimePublicationSlotState::Published(Box::new(
            RuntimeInstance::from_promotion(owner, generation, None),
        ));
        state.publication = RuntimePublicationPhase::Published;
        Ok(())
    }
}

pub struct RuntimeCandidateFailure {
    retirement: Box<RuntimeRetirement>,
}

impl Debug for RuntimeCandidateFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeCandidateFailure")
            .field("generation", &self.retirement.generation())
            .field("fault", &self.fault())
            .field("retirement", &self.retirement_state())
            .finish_non_exhaustive()
    }
}

impl Display for RuntimeCandidateFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "runtime candidate startup failed: kind={:?}, source={}",
            self.fault().kind(),
            self.fault().source()
        )
    }
}

impl std::error::Error for RuntimeCandidateFailure {}

impl RuntimeCandidateFailure {
    fn new(mut owner: RuntimeOwner, generation: RuntimeGeneration, fault: RuntimeFault) -> Self {
        owner.reporter().record_canonical(fault);
        owner.begin_close();
        Self {
            retirement: Box::new(RuntimeRetirement::new(owner, generation)),
        }
    }

    #[must_use]
    pub fn fault(&self) -> &RuntimeFault {
        self.retirement
            .fault()
            .expect("candidate failure retirement always retains its startup fault")
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.retirement.close_evidence()
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        self.retirement.drive_retirement()
    }

    #[must_use]
    pub fn fault_reporter(&self) -> RuntimeFaultReporter {
        self.retirement.fault_reporter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCandidateRetirementState {
    Retiring,
    RetirementIncomplete,
    Retired,
}

#[must_use = "runtime retirement owns close obligations until retirement completes"]
pub struct RuntimeRetirement {
    owner: RuntimeOwner,
    generation: RuntimeGeneration,
}

impl Debug for RuntimeRetirement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeRetirement")
            .field("generation", &self.generation)
            .field("state", &self.retirement_state())
            .field("fault", &self.fault())
            .finish_non_exhaustive()
    }
}

impl RuntimeRetirement {
    fn new(owner: RuntimeOwner, generation: RuntimeGeneration) -> Self {
        Self { owner, generation }
    }

    #[must_use]
    pub const fn generation(&self) -> RuntimeGeneration {
        self.generation
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.owner.close_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.owner.close_evidence()
    }

    #[must_use]
    pub fn fault_reporter(&self) -> RuntimeFaultReporter {
        self.owner.reporter().clone()
    }

    #[must_use]
    pub fn fault(&self) -> Option<&RuntimeFault> {
        self.owner.reporter().fault_ref()
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        if self.owner.close_state() == RuntimeCandidateRetirementState::RetirementIncomplete {
            self.owner.retry_close();
        }
        self.owner.drive_close();
        self.retirement_state()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControl {
    Pause,
    Resume,
    StepFixedTick,
    Stop,
    RetryClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeControlTicket {
    generation: RuntimeGeneration,
    sequence: u64,
}

impl RuntimeControlTicket {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn generation(self) -> RuntimeGeneration {
        self.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlFailure {
    SupersededByStop,
    RuntimeFaulted,
    CloseFailed,
    CloseIncomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlStatus {
    Pending,
    Applied,
    Failed(RuntimeControlFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlRejection {
    Busy,
    InvalidState {
        state: RuntimeState,
        control: RuntimeControl,
    },
    TicketExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlRequestResult {
    Accepted(RuntimeControlTicket),
    Rejected(RuntimeControlRejection),
}

#[derive(Debug, Clone, Copy)]
struct PendingRuntimeControl {
    ticket: RuntimeControlTicket,
    control: RuntimeControl,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeControlRecord {
    ticket: RuntimeControlTicket,
    status: RuntimeControlStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDriveOutcome {
    state: RuntimeState,
    frame: Option<AppFrameOutcome>,
}

impl RuntimeDriveOutcome {
    #[must_use]
    pub const fn state(self) -> RuntimeState {
        self.state
    }

    #[must_use]
    pub const fn frame(self) -> Option<AppFrameOutcome> {
        self.frame
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeDriveError {
    #[error("runtime faulted")]
    Faulted { fault: RuntimeFault },
}

impl RuntimeDriveError {
    #[must_use]
    pub const fn fault(&self) -> &RuntimeFault {
        match self {
            Self::Faulted { fault } => fault,
        }
    }
}

pub struct RuntimeInstance {
    owner: RuntimeOwner,
    generation: RuntimeGeneration,
    state: RuntimeState,
    pending_control: Option<PendingRuntimeControl>,
    control_results: VecDeque<RuntimeControlRecord>,
    next_control_ticket: u64,
    stop_ticket: Option<RuntimeControlTicket>,
    active_close_ticket: Option<RuntimeControlTicket>,
}

impl Debug for RuntimeInstance {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeInstance")
            .field("generation", &self.generation)
            .field("state", &self.state())
            .field("fault", &self.fault())
            .finish_non_exhaustive()
    }
}

impl RuntimeInstance {
    fn from_promotion(
        owner: RuntimeOwner,
        generation: RuntimeGeneration,
        fault: Option<RuntimeFault>,
    ) -> Self {
        Self {
            owner,
            generation,
            state: if fault.is_some() {
                RuntimeState::Faulted
            } else {
                RuntimeState::Running
            },
            pending_control: None,
            control_results: VecDeque::new(),
            next_control_ticket: 1,
            stop_ticket: None,
            active_close_ticket: None,
        }
    }

    #[must_use]
    pub const fn generation(&self) -> RuntimeGeneration {
        self.generation
    }

    #[must_use]
    pub fn state(&self) -> RuntimeState {
        if matches!(
            self.state,
            RuntimeState::Running | RuntimeState::Paused | RuntimeState::Stepping
        ) && self.fault().is_some()
        {
            RuntimeState::Faulted
        } else {
            self.state
        }
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.owner.close_evidence()
    }

    #[must_use]
    pub fn fault(&self) -> Option<&RuntimeFault> {
        self.owner.reporter().fault_ref()
    }

    #[must_use]
    pub fn fault_reporter(&self) -> RuntimeFaultReporter {
        self.owner.reporter().clone()
    }

    pub fn begin_retirement(mut self) -> RuntimeRetirement {
        self.owner.begin_close();
        RuntimeRetirement::new(self.owner, self.generation)
    }

    #[must_use]
    pub fn world(&self) -> &World {
        self.owner.app.world()
    }

    #[cfg(test)]
    pub(crate) fn world_mut_for_tests(&mut self) -> &mut World {
        &mut self.owner.app.world
    }

    /// Grants a platform or Host one short-lived managed runtime scope.
    ///
    /// The scope exposes immutable World inspection plus guarded mutable resource access.
    /// Runtime-managed fault resources cannot be selected through that mutable surface. A
    /// previously faulted runtime still permits this scope so the driver can retire surfaces and
    /// other owned services; the existing fault remains sticky and observable.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeScopeError::Unavailable`] while stepping or after the runtime is stopped.
    /// Returns [`RuntimeScopeError::Faulted`] when healthy-scope authority was replaced or the
    /// operation reports a new canonical runtime fault.
    pub fn with_driver_scope<R>(
        &mut self,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> R,
    ) -> Result<R, RuntimeScopeError> {
        let state = self.state();
        if matches!(state, RuntimeState::Stepping | RuntimeState::Stopped) {
            return Err(RuntimeScopeError::Unavailable { state });
        }
        self.owner
            .capture_driver_scope(|world| {
                let mut scope = RuntimeDriverScope { world };
                operation(&mut scope)
            })
            .map_err(|fault| RuntimeScopeError::Faulted { fault })
    }

    pub fn request_control(&mut self, control: RuntimeControl) -> RuntimeControlRequestResult {
        let state = self.state();
        if control == RuntimeControl::Stop
            && let Some(ticket) = self.stop_ticket
            && matches!(
                state,
                RuntimeState::Running
                    | RuntimeState::Paused
                    | RuntimeState::Faulted
                    | RuntimeState::Stopping
            )
        {
            return RuntimeControlRequestResult::Accepted(ticket);
        }

        if let Some(pending) = self.pending_control {
            if control != RuntimeControl::Stop {
                return RuntimeControlRequestResult::Rejected(RuntimeControlRejection::Busy);
            }
            if !self.accepts(RuntimeControl::Stop) {
                return RuntimeControlRequestResult::Rejected(
                    RuntimeControlRejection::InvalidState {
                        state: self.state,
                        control,
                    },
                );
            }
            self.set_control_status(
                pending.ticket,
                RuntimeControlStatus::Failed(RuntimeControlFailure::SupersededByStop),
            );
            self.pending_control = None;
        }

        let state = self.state();
        if !self.accepts(control) {
            return RuntimeControlRequestResult::Rejected(RuntimeControlRejection::InvalidState {
                state,
                control,
            });
        }

        let Some(next) = self.next_control_ticket.checked_add(1) else {
            return RuntimeControlRequestResult::Rejected(RuntimeControlRejection::TicketExhausted);
        };
        let ticket = RuntimeControlTicket {
            generation: self.generation,
            sequence: self.next_control_ticket,
        };
        self.next_control_ticket = next;
        self.pending_control = Some(PendingRuntimeControl { ticket, control });
        self.push_control_record(ticket, RuntimeControlStatus::Pending);
        if control == RuntimeControl::Stop {
            self.stop_ticket = Some(ticket);
        }
        RuntimeControlRequestResult::Accepted(ticket)
    }

    #[must_use]
    pub fn control_status(&self, ticket: RuntimeControlTicket) -> Option<RuntimeControlStatus> {
        if ticket.generation != self.generation {
            return None;
        }
        self.control_results
            .iter()
            .rev()
            .find(|record| record.ticket == ticket)
            .map(|record| record.status)
    }

    pub fn drive(
        &mut self,
        real_delta: Duration,
    ) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        if self.state != RuntimeState::Stopped
            && let Some(fault) = self.owner.reporter().fault()
        {
            if matches!(self.state, RuntimeState::Running | RuntimeState::Paused)
                && !self
                    .pending_control
                    .is_some_and(|pending| pending.control == RuntimeControl::Stop)
            {
                if let Some(pending) = self.pending_control.take() {
                    self.set_control_status(
                        pending.ticket,
                        RuntimeControlStatus::Failed(RuntimeControlFailure::RuntimeFaulted),
                    );
                }
                return Err(self.enter_fault(fault));
            }
        }

        if let Some(pending) = self.pending_control.take() {
            match pending.control {
                RuntimeControl::Pause => {
                    if let Err(error) = self.set_frame_mode(RuntimeFrameMode::Paused) {
                        self.set_control_status(
                            pending.ticket,
                            RuntimeControlStatus::Failed(RuntimeControlFailure::RuntimeFaulted),
                        );
                        return Err(error);
                    }
                    self.state = RuntimeState::Paused;
                    self.set_control_status(pending.ticket, RuntimeControlStatus::Applied);
                }
                RuntimeControl::Resume => {
                    if let Err(error) = self.set_frame_mode(RuntimeFrameMode::Running) {
                        self.set_control_status(
                            pending.ticket,
                            RuntimeControlStatus::Failed(RuntimeControlFailure::RuntimeFaulted),
                        );
                        return Err(error);
                    }
                    self.state = RuntimeState::Running;
                    self.set_control_status(pending.ticket, RuntimeControlStatus::Applied);
                }
                RuntimeControl::StepFixedTick => {
                    self.state = RuntimeState::Stepping;
                    let frame = self.drive_app_exact_step(real_delta);
                    return match frame {
                        Ok(frame) => {
                            self.state = RuntimeState::Paused;
                            self.set_control_status(pending.ticket, RuntimeControlStatus::Applied);
                            Ok(RuntimeDriveOutcome {
                                state: self.state,
                                frame: Some(frame),
                            })
                        }
                        Err(error) => {
                            self.set_control_status(
                                pending.ticket,
                                RuntimeControlStatus::Failed(RuntimeControlFailure::RuntimeFaulted),
                            );
                            Err(error)
                        }
                    };
                }
                RuntimeControl::Stop => {
                    self.state = RuntimeState::Stopping;
                    self.owner.begin_close();
                    self.active_close_ticket = Some(pending.ticket);
                    return Ok(RuntimeDriveOutcome {
                        state: self.state,
                        frame: None,
                    });
                }
                RuntimeControl::RetryClose => {
                    self.state = RuntimeState::Stopping;
                    self.owner.retry_close();
                    self.active_close_ticket = Some(pending.ticket);
                    return Ok(RuntimeDriveOutcome {
                        state: self.state,
                        frame: None,
                    });
                }
            }
        }

        match self.state {
            RuntimeState::Running => {
                let frame = self.drive_app_frame(real_delta, RuntimeFrameMode::Running)?;
                Ok(RuntimeDriveOutcome {
                    state: self.state,
                    frame: Some(frame),
                })
            }
            RuntimeState::Paused => {
                let frame = self.drive_app_frame(real_delta, RuntimeFrameMode::Paused)?;
                Ok(RuntimeDriveOutcome {
                    state: self.state,
                    frame: Some(frame),
                })
            }
            RuntimeState::Faulted => Err(RuntimeDriveError::Faulted {
                fault: self
                    .fault()
                    .cloned()
                    .expect("a faulted runtime retains its first fault"),
            }),
            RuntimeState::Stopping => {
                self.owner.drive_close();
                self.state = match self.owner.close_state() {
                    RuntimeCandidateRetirementState::Retiring => RuntimeState::Stopping,
                    RuntimeCandidateRetirementState::RetirementIncomplete => {
                        RuntimeState::CloseIncomplete
                    }
                    RuntimeCandidateRetirementState::Retired => RuntimeState::Stopped,
                };
                self.finish_close_control_status();
                Ok(RuntimeDriveOutcome {
                    state: self.state,
                    frame: None,
                })
            }
            RuntimeState::CloseIncomplete | RuntimeState::Stopped => Ok(RuntimeDriveOutcome {
                state: self.state,
                frame: None,
            }),
            RuntimeState::Stepping => unreachable!("exact stepping completes inside one drive"),
        }
    }

    fn accepts(&self, control: RuntimeControl) -> bool {
        matches!(
            (self.state(), control),
            (
                RuntimeState::Running,
                RuntimeControl::Pause | RuntimeControl::Stop
            ) | (
                RuntimeState::Paused,
                RuntimeControl::Resume | RuntimeControl::StepFixedTick | RuntimeControl::Stop
            ) | (RuntimeState::Faulted, RuntimeControl::Stop)
                | (RuntimeState::CloseIncomplete, RuntimeControl::RetryClose)
        )
    }

    fn set_frame_mode(&mut self, mode: RuntimeFrameMode) -> Result<(), RuntimeDriveError> {
        let Some(mut settings) = self
            .owner
            .app
            .world
            .get_resource_mut::<RuntimeTimeSettings>()
        else {
            return Err(self.enter_fault(RuntimeFault::engine(
                RuntimeFaultKind::AppFrame,
                "nara.app.missing-runtime-time-settings",
            )));
        };
        settings.set_paused(mode.is_paused());
        Ok(())
    }

    fn drive_app_frame(
        &mut self,
        real_delta: Duration,
        mode: RuntimeFrameMode,
    ) -> Result<AppFrameOutcome, RuntimeDriveError> {
        let bridge_epoch = match self.owner.begin_fault_bridge_epoch() {
            Ok(epoch) => epoch,
            Err(fault) => return Err(self.enter_fault(fault)),
        };
        let reporter = self.owner.reporter().clone();
        let result = with_runtime_system_fault_capture(&reporter, || match mode {
            RuntimeFrameMode::Running => self.owner.app.run_managed_frame(real_delta),
            RuntimeFrameMode::Paused => self.owner.app.run_paused_frame(real_delta),
        });
        match result {
            Ok(result) => self.finish_app_drive(result, bridge_epoch),
            Err(fault) => Err(self.enter_fault(fault)),
        }
    }

    fn drive_app_exact_step(
        &mut self,
        real_delta: Duration,
    ) -> Result<AppFrameOutcome, RuntimeDriveError> {
        let bridge_epoch = match self.owner.begin_fault_bridge_epoch() {
            Ok(epoch) => epoch,
            Err(fault) => return Err(self.enter_fault(fault)),
        };
        let reporter = self.owner.reporter().clone();
        let result = with_runtime_system_fault_capture(&reporter, || {
            self.owner.app.run_exact_fixed_tick(real_delta)
        });
        match result {
            Ok(result) => self.finish_app_drive(result, bridge_epoch),
            Err(fault) => Err(self.enter_fault(fault)),
        }
    }

    fn finish_close_control_status(&mut self) {
        let Some(ticket) = self.active_close_ticket else {
            return;
        };
        let status = match self.state {
            RuntimeState::Stopped if self.owner.close_evidence().plugin_shutdown_failed() => {
                RuntimeControlStatus::Failed(RuntimeControlFailure::CloseFailed)
            }
            RuntimeState::Stopped => RuntimeControlStatus::Applied,
            RuntimeState::CloseIncomplete => {
                RuntimeControlStatus::Failed(RuntimeControlFailure::CloseIncomplete)
            }
            _ => return,
        };
        self.set_control_status(ticket, status);
        self.active_close_ticket = None;
    }

    fn finish_app_drive(
        &mut self,
        result: Result<AppFrameOutcome, AppRunError>,
        bridge_epoch: RuntimeFaultBridgeEpoch,
    ) -> Result<AppFrameOutcome, RuntimeDriveError> {
        if let Err(error) = result {
            return Err(self.enter_fault(RuntimeFault::app(error)));
        }
        if let Err(fault) = self.owner.validate_fault_bridge_epoch(bridge_epoch) {
            return Err(self.enter_fault(fault));
        }
        if let Some(fault) = self.owner.reporter().fault() {
            return Err(self.enter_fault(fault));
        }
        Ok(result.expect("the app drive result was checked above"))
    }

    fn enter_fault(&mut self, fault: RuntimeFault) -> RuntimeDriveError {
        let first = self.owner.reporter().record_canonical(fault);
        self.state = RuntimeState::Faulted;
        RuntimeDriveError::Faulted { fault: first }
    }

    fn push_control_record(&mut self, ticket: RuntimeControlTicket, status: RuntimeControlStatus) {
        if self.control_results.len() == MAX_RETAINED_CONTROL_RESULTS {
            self.control_results.pop_front();
        }
        self.control_results
            .push_back(RuntimeControlRecord { ticket, status });
    }

    fn set_control_status(&mut self, ticket: RuntimeControlTicket, status: RuntimeControlStatus) {
        if let Some(record) = self
            .control_results
            .iter_mut()
            .rev()
            .find(|record| record.ticket == ticket)
        {
            record.status = status;
        }
    }
}

struct RuntimeOwnedValue<T>(Option<T>);

impl<T> RuntimeOwnedValue<T> {
    fn new(value: T) -> Self {
        Self(Some(value))
    }
}

impl<T> Deref for RuntimeOwnedValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("runtime-owned value remains present while the owner is usable")
    }
}

impl<T> DerefMut for RuntimeOwnedValue<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("runtime-owned value remains present while the owner is usable")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginShutdownState {
    Pending,
    Complete,
}

#[derive(Debug, Clone, Copy)]
struct OwnerCloseAttempt {
    deadline: Instant,
    plugin_shutdown: PluginShutdownState,
}

#[derive(Debug, Clone, Copy)]
enum OwnerCloseState {
    Open,
    Closing(OwnerCloseAttempt),
    Incomplete {
        plugin_shutdown: PluginShutdownState,
    },
    Complete,
}

struct RuntimeOwner {
    state: Option<RuntimeOwnerState>,
    quarantine_accounted: bool,
}

struct RuntimeOwnerState {
    app: RuntimeOwnedValue<App>,
    obligations: RuntimeOwnedValue<RuntimeObligationLedger>,
    reporter: RuntimeFaultReporter,
    close_policy: RuntimeClosePolicy,
    close_evidence: RuntimeCloseEvidence,
    close_state: OwnerCloseState,
}

impl RuntimeOwner {
    fn new(
        app: App,
        obligations: RuntimeObligationLedger,
        reporter: RuntimeFaultReporter,
        close_policy: RuntimeClosePolicy,
    ) -> Self {
        Self {
            state: Some(RuntimeOwnerState {
                app: RuntimeOwnedValue::new(app),
                obligations: RuntimeOwnedValue::new(obligations),
                reporter,
                close_policy,
                close_evidence: RuntimeCloseEvidence::default(),
                close_state: OwnerCloseState::Open,
            }),
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
    fn begin_close(&mut self) {
        if matches!(self.close_state, OwnerCloseState::Open) {
            self.reporter.mark_closing();
            self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                deadline: deadline_after(self.close_policy.timeout()),
                plugin_shutdown: PluginShutdownState::Pending,
            });
        }
    }

    fn retry_close(&mut self) {
        match self.close_state {
            OwnerCloseState::Open => self.begin_close(),
            OwnerCloseState::Incomplete { plugin_shutdown } => {
                self.reporter.mark_closing();
                self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                    deadline: deadline_after(self.close_policy.timeout()),
                    plugin_shutdown,
                });
            }
            OwnerCloseState::Closing(_) | OwnerCloseState::Complete => {}
        }
    }

    fn drive_close(&mut self) {
        let OwnerCloseState::Closing(mut attempt) = self.close_state else {
            return;
        };

        let bridge_epoch = match self.begin_fault_bridge_epoch() {
            Ok(epoch) => Some(epoch),
            Err(fault) => {
                self.reporter.record_canonical(fault);
                None
            }
        };
        let reporter = self.reporter.clone();
        let close_pass = with_runtime_system_fault_capture(&reporter, || {
            if attempt.plugin_shutdown == PluginShutdownState::Pending {
                attempt.plugin_shutdown = match self.app.shutdown_plugins() {
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
                .drive_close_once_with_evidence(&mut self.app.world, &mut self.close_evidence);
            (attempt, participant_failed)
        });
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
            self.reporter.record_canonical(fault);
        }

        let all_complete = self.obligations.is_close_complete();
        if all_complete && attempt.plugin_shutdown == PluginShutdownState::Complete {
            self.reporter.mark_closed();
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

    fn reporter(&self) -> &RuntimeFaultReporter {
        &self.reporter
    }

    fn begin_fault_bridge_epoch(&mut self) -> Result<RuntimeFaultBridgeEpoch, RuntimeFault> {
        self.validate_fault_bridge_authority()?;
        let revision = self.app.world.resource::<RuntimeFaultBridgeRevision>().0;
        let reporter = self.reporter.clone();
        let fault_before = reporter.fault();
        with_runtime_system_fault_capture(&reporter, || {
            self.app.world.increment_change_tick();
            self.app.world.check_change_ticks();
        })?;
        self.validate_fault_bridge_authority()?;
        if self.app.world.resource::<RuntimeFaultBridgeRevision>().0 != revision {
            return Err(runtime_fault_bridge_authority_fault());
        }
        if fault_before.is_none()
            && let Some(fault) = reporter.fault()
        {
            return Err(fault);
        }
        RuntimeFaultBridgeEpoch::capture(&self.app.world)
            .ok_or_else(runtime_fault_bridge_authority_fault)
    }

    fn validate_fault_bridge_epoch(
        &self,
        expected: RuntimeFaultBridgeEpoch,
    ) -> Result<(), RuntimeFault> {
        self.validate_fault_bridge_authority()?;
        if RuntimeFaultBridgeEpoch::capture(&self.app.world) == Some(expected) {
            Ok(())
        } else {
            Err(runtime_fault_bridge_authority_fault())
        }
    }

    fn validate_fault_bridge_authority(&self) -> Result<(), RuntimeFault> {
        validate_runtime_fault_bridge_authority(&self.app.world, &self.reporter)
    }

    fn capture_driver_scope<R>(
        &mut self,
        operation: impl FnOnce(&mut World) -> R,
    ) -> Result<R, RuntimeFault> {
        let reporter = self.reporter.clone();
        let fault_before = reporter.fault();
        let bridge_epoch = self
            .begin_fault_bridge_epoch()
            .map_err(|fault| reporter.record_canonical(fault))?;

        let result =
            match with_runtime_system_fault_capture(&reporter, || operation(&mut self.app.world)) {
                Ok(result) => result,
                Err(fault) => return Err(reporter.record_canonical(fault)),
            };

        if let Err(fault) = self.validate_fault_bridge_epoch(bridge_epoch) {
            return Err(reporter.record_canonical(fault));
        }
        if fault_before.is_none() {
            if let Some(fault) = reporter.fault() {
                return Err(fault);
            }
        }
        Ok(result)
    }

    fn close_evidence(&self) -> &RuntimeCloseEvidence {
        &self.close_evidence
    }

    fn close_state(&self) -> RuntimeCandidateRetirementState {
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
        guard.begin_close();
        guard.drive_close();
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
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
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
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS).then_some(current + 1)
        })
        .is_ok()
}

fn increment_atomic_u64(counter: &AtomicU64) {
    if counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .is_err()
    {
        std::process::abort();
    }
}

fn allocate_generation() -> Result<RuntimeGeneration, RuntimeAdmissionError> {
    NEXT_RUNTIME_GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map(RuntimeGeneration)
        .map_err(|_| RuntimeAdmissionError::GenerationExhausted)
}

fn deadline_after(timeout: Duration) -> Instant {
    Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now)
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn runtime_fault_bridge_authority_fault() -> RuntimeFault {
    RuntimeFault::engine(
        RuntimeFaultKind::FaultReporterAuthority,
        "nara.app.runtime-fault-reporter",
    )
}

struct ActiveRuntimeReporterGuard<'lock> {
    _schedule_authority: MutexGuard<'lock, ()>,
}

impl Drop for ActiveRuntimeReporterGuard<'_> {
    fn drop(&mut self) {
        *lock_unpoisoned(&ACTIVE_RUNTIME_REPORTER) = None;
    }
}

fn with_runtime_system_fault_capture<R>(
    reporter: &RuntimeFaultReporter,
    operation: impl FnOnce() -> R,
) -> Result<R, RuntimeFault> {
    let schedule_authority = match RUNTIME_SCHEDULE_AUTHORITY.try_lock() {
        Ok(authority) => authority,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return Err(RuntimeFault::engine(
                RuntimeFaultKind::ScheduleAuthority,
                "nara.app.runtime-schedule-authority",
            ));
        }
    };
    *lock_unpoisoned(&ACTIVE_RUNTIME_REPORTER) = Some(reporter.clone());
    let _guard = ActiveRuntimeReporterGuard {
        _schedule_authority: schedule_authority,
    };
    Ok(operation())
}

pub(crate) fn runtime_system_error_handler(error: BevyError, context: ErrorContext) {
    let severity = error.severity();
    if severity == Severity::Error {
        let kind = match &context {
            ErrorContext::System { .. } => RuntimeFaultKind::System,
            ErrorContext::RunCondition { .. } => RuntimeFaultKind::RunCondition,
            ErrorContext::Command { .. } => RuntimeFaultKind::Command,
            ErrorContext::Observer { .. } => RuntimeFaultKind::Observer,
        };
        if let Some(reporter) = lock_unpoisoned(&ACTIVE_RUNTIME_REPORTER).as_ref() {
            reporter.report(RuntimeFault::engine(kind, "nara.ecs.fallible-execution"));
            return;
        }
    }
    match_severity(error, context);
}
