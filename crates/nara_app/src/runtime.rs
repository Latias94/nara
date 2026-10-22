use std::{
    any::{TypeId, type_name},
    collections::VecDeque,
    fmt::{self, Debug, Display, Formatter},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bevy_ecs::error::{BevyError, ErrorHandler, FallbackErrorHandler};
use nara_ecs::{
    Command, Mut, Resource, World,
    change_detection::Tick,
    component::Mutable,
    lifecycle::HookContext,
    schedule::{InternedScheduleLabel, ScheduleLabel},
    world::DeferredWorld,
};
use thiserror::Error;

use crate::{
    App, AppFrameOutcome, AppRunError, PluginFailureReport, PluginLifecycleState,
    RuntimeTimeSettings, SealedApp,
};

mod close;
mod fault_route;
mod owner;

pub use close::{
    RuntimeClosePendingParticipant, RuntimeObligationLedger, RuntimeObligationLedgerError,
    RuntimeObligationRegistrationError,
};
use fault_route::{FaultRouteReservation, FaultRouteToken};
pub(super) use owner::{RuntimeOwner, RuntimeOwnerState};
pub use owner::{RuntimeQuarantineStatus, drive_runtime_quarantine, runtime_quarantine_status};

const MAX_RETAINED_CONTROL_RESULTS: usize = 32;
const DEFAULT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Hidden product plumbing for an engine-owned authority that must be checked at App and managed
/// runtime safe points. This is intentionally one callback rather than a public event bus or a
/// general extension registry; the owning domain keeps the identity and fault reporter it needs.
#[doc(hidden)]
pub type __RuntimeAuthorityValidator =
    dyn Fn(&World, Option<RuntimeGeneration>) -> Result<(), RuntimeFault> + Send + Sync;

#[derive(Debug, Default, Resource)]
struct RuntimeFaultBridgeRevision(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeFaultBridgeIdentity {
    revision: u64,
}

impl RuntimeFaultBridgeIdentity {
    pub(crate) fn capture(world: &World) -> Option<Self> {
        let revision = world.get_resource::<RuntimeFaultBridgeRevision>()?.0;
        world.get_resource::<RuntimeFaultReporter>()?;
        world.get_resource::<FallbackErrorHandler>()?;
        Some(Self { revision })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeFaultBridgeEpoch {
    revision: u64,
    observed_at: Tick,
}

impl RuntimeFaultBridgeEpoch {
    pub(crate) fn capture(world: &World) -> Option<Self> {
        let revision = world.get_resource::<RuntimeFaultBridgeRevision>()?.0;
        world.get_resource::<RuntimeFaultReporter>()?;
        world.get_resource::<FallbackErrorHandler>()?;
        Some(Self {
            revision,
            observed_at: world.read_change_tick(),
        })
    }

    pub(crate) fn is_unchanged(self, world: &World) -> bool {
        let Some(revision) = world.get_resource::<RuntimeFaultBridgeRevision>() else {
            return false;
        };
        let Some(reporter) = world.get_resource_change_ticks::<RuntimeFaultReporter>() else {
            return false;
        };
        let Some(handler) = world.get_resource_change_ticks::<FallbackErrorHandler>() else {
            return false;
        };
        let current = world.read_change_tick();
        revision.0 == self.revision
            && !reporter.added.is_newer_than(self.observed_at, current)
            && !reporter.changed.is_newer_than(self.observed_at, current)
            && !handler.added.is_newer_than(self.observed_at, current)
            && !handler.changed.is_newer_than(self.observed_at, current)
    }
}

pub(crate) fn run_runtime_fault_bridge_maintenance(
    world: &mut World,
    maintenance: impl FnOnce(&mut World),
) -> Result<(), RuntimeFault> {
    let epoch =
        RuntimeFaultBridgeEpoch::capture(world).ok_or_else(runtime_fault_bridge_authority_fault)?;
    maintenance(world);
    if epoch.is_unchanged(world) {
        Ok(())
    } else {
        Err(runtime_fault_bridge_authority_fault())
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
    world.insert_resource(FallbackErrorHandler::default());
}

pub(crate) fn validate_managed_runtime_authority(
    world: &World,
    reporter: &RuntimeFaultReporter,
    expected_handler: Option<ErrorHandler>,
    expected_bridge: Option<RuntimeFaultBridgeIdentity>,
    generation: RuntimeGeneration,
    authority: Option<&Arc<__RuntimeAuthorityValidator>>,
) -> Result<(), RuntimeFault> {
    if let Some(fault) = reporter.fault() {
        return Err(fault);
    }
    let fault = (world.get_resource::<RuntimeGeneration>() != Some(&generation))
        .then(runtime_fault_bridge_authority_fault)
        .or_else(|| authority.and_then(|validator| validator(world, Some(generation)).err()))
        .or_else(|| {
            validate_runtime_fault_bridge_authority(
                world,
                reporter,
                expected_handler,
                expected_bridge,
            )
            .err()
        });
    let Some(fault) = fault else {
        return Ok(());
    };
    let canonical = reporter.record_canonical(fault.clone());
    Err(canonical)
}

pub(crate) fn record_runtime_fault_bridge_authority_fault(
    reporter: &RuntimeFaultReporter,
) -> RuntimeFault {
    reporter.record_canonical(runtime_fault_bridge_authority_fault())
}

fn validate_runtime_fault_bridge_authority(
    world: &World,
    reporter: &RuntimeFaultReporter,
    expected_handler: Option<ErrorHandler>,
    expected_bridge: Option<RuntimeFaultBridgeIdentity>,
) -> Result<(), RuntimeFault> {
    let reporter_valid = world
        .get_resource::<RuntimeFaultReporter>()
        .is_some_and(|world_reporter| world_reporter.has_authority(reporter));
    let handler_valid = world
        .get_resource::<FallbackErrorHandler>()
        .zip(expected_handler)
        .is_some_and(|(handler, expected)| std::ptr::fn_addr_eq(handler.0, expected));
    let bridge_valid = expected_bridge
        .is_some_and(|expected| RuntimeFaultBridgeIdentity::capture(world) == Some(expected));
    if reporter_valid && handler_valid && bridge_valid {
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
    RuntimeAuthority,
    System,
    RunCondition,
    Command,
    Observer,
    GameplayLifecycle,
    LocalIntentLoss,
    RequiredTask,
    RequiredService,
}

/// Validation failure for source-authored runtime fault metadata.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFaultDetailError {
    #[error("runtime fault code is invalid")]
    InvalidCode,
    #[error("runtime fault summary is invalid")]
    InvalidSummary,
    #[error("runtime fault origin is invalid")]
    InvalidOrigin,
}

/// Bounded, source-authored diagnostic metadata for an engine-owned runtime fault.
///
/// All fields are static so runtime values, paths, credentials, and third-party error text cannot
/// be retained accidentally. Construction also rejects oversized values, invalid identifiers,
/// control and formatting characters, absolute paths, URLs, and secret-shaped text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeFaultDetail {
    code: &'static str,
    summary: &'static str,
    origin: &'static str,
}

impl RuntimeFaultDetail {
    /// The maximum encoded byte length accepted for a diagnostic code.
    pub const MAX_CODE_BYTES: usize = crate::failure_metadata::MAX_CODE_BYTES;
    /// The maximum encoded byte length accepted for a diagnostic summary.
    pub const MAX_SUMMARY_BYTES: usize = crate::failure_metadata::MAX_SUMMARY_BYTES;
    /// The maximum encoded byte length accepted for a producer origin.
    pub const MAX_ORIGIN_BYTES: usize = 96;

    /// Creates validated metadata from source-authored static strings.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeFaultDetailError`] when any field violates its bounded public-data
    /// contract.
    pub fn new(
        code: &'static str,
        summary: &'static str,
        origin: &'static str,
    ) -> Result<Self, RuntimeFaultDetailError> {
        if !crate::failure_metadata::is_valid_identifier(code, Self::MAX_CODE_BYTES, false) {
            return Err(RuntimeFaultDetailError::InvalidCode);
        }
        if !crate::failure_metadata::is_valid_summary(summary) {
            return Err(RuntimeFaultDetailError::InvalidSummary);
        }
        if !crate::failure_metadata::is_valid_identifier(origin, Self::MAX_ORIGIN_BYTES, true) {
            return Err(RuntimeFaultDetailError::InvalidOrigin);
        }
        Ok(Self {
            code,
            summary,
            origin,
        })
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        self.summary
    }

    #[must_use]
    pub const fn origin(self) -> &'static str {
        self.origin
    }

    /// Converts this validated metadata into a non-panicking Bevy system error.
    #[must_use]
    pub fn into_bevy_error(self) -> BevyError {
        BevyError::error(RuntimeExecutionError { detail: self })
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("engine-owned runtime execution failed")]
pub(crate) struct RuntimeExecutionError {
    detail: RuntimeFaultDetail,
}

impl RuntimeExecutionError {
    #[must_use]
    pub(crate) const fn detail(self) -> RuntimeFaultDetail {
        self.detail
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeFault {
    kind: RuntimeFaultKind,
    source: &'static str,
    detail: Option<RuntimeFaultDetail>,
    app_error: Option<Box<AppRunError>>,
}

impl RuntimeFault {
    #[must_use]
    pub const fn engine(kind: RuntimeFaultKind, source: &'static str) -> Self {
        Self {
            kind,
            source,
            detail: None,
            app_error: None,
        }
    }

    /// Creates an engine fault carrying validated, source-authored diagnostic metadata.
    #[must_use]
    pub const fn engine_with_detail(
        kind: RuntimeFaultKind,
        source: &'static str,
        detail: RuntimeFaultDetail,
    ) -> Self {
        Self {
            kind,
            source,
            detail: Some(detail),
            app_error: None,
        }
    }

    #[must_use]
    pub(crate) fn app(error: AppRunError) -> Self {
        let (kind, source, detail) = error.runtime_fault_parts().unwrap_or((
            RuntimeFaultKind::AppFrame,
            "nara.app.frame",
            None,
        ));
        Self {
            kind,
            source,
            detail,
            app_error: Some(Box::new(error)),
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
    pub const fn detail(&self) -> Option<RuntimeFaultDetail> {
        self.detail
    }

    #[must_use]
    pub fn app_error(&self) -> Option<&AppRunError> {
        self.app_error.as_deref()
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
    /// All physical responsibility ended, but teardown produced final error evidence.
    CompleteWithError {
        code: &'static str,
    },
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
    pending_participants: Vec<RuntimeClosePendingParticipant>,
}

impl RuntimeCloseEvidence {
    /// Historical error and deadline evidence, retained after a successful retry.
    #[must_use]
    pub fn causes(&self) -> &[RuntimeCloseCause] {
        &self.causes
    }

    /// Unfinished participants at the latest close drive, including terminal live obligations.
    ///
    /// Closing starts with every participant in `Begin`. Completed participants are removed,
    /// including those returning `CompleteWithError`; pending work does not create an error cause.
    #[must_use]
    pub fn pending_participants(&self) -> &[RuntimeClosePendingParticipant] {
        &self.pending_participants
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.causes.is_empty() && self.pending_participants.is_empty()
    }

    #[must_use]
    pub fn plugin_shutdown_failed(&self) -> bool {
        self.causes.contains(&RuntimeCloseCause::PluginShutdown)
    }

    /// Whether completed teardown retained a final failure, independent of physical completion.
    /// Historical retryable errors and deadlines do not invalidate a later successful close.
    #[must_use]
    pub fn has_terminal_failure(&self) -> bool {
        self.causes.iter().any(|cause| {
            matches!(cause, RuntimeCloseCause::PluginShutdown)
                || matches!(
                    cause,
                    RuntimeCloseCause::ParticipantError {
                        disposition: RuntimeCloseErrorDisposition::Terminal,
                        ..
                    }
                )
        })
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

/// First-party platform bridge for one typed operation under managed fault authority.
#[doc(hidden)]
pub struct RuntimeDriverScope<'world> {
    world: &'world mut World,
    state: RuntimeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriverScopeAuthorityPolicy {
    Full,
    FaultRetirement,
}

/// First-party workspace plumbing for a typed managed-runtime operation.
///
/// Implementations define the complete input and output surface for their own resource. Rust's
/// orphan rules prevent downstream adapters from adding driver access to an engine-owned resource.
/// This is intentionally hidden and may be replaced without a compatibility migration; it is not
/// the public shared Adapter contract tracked by OQ-038.
/// Coordinated operations require explicit configuration through [`App::__register_driver_port`].
#[doc(hidden)]
pub trait __RuntimeDriverPort: Resource<Mutability = Mutable> {
    type Input;
    type Output;

    #[must_use]
    fn accepts_driver_state(state: RuntimeState) -> bool {
        matches!(state, RuntimeState::Running | RuntimeState::Paused)
    }

    #[must_use]
    fn accepts_driver_input(state: RuntimeState, _input: &Self::Input) -> bool {
        Self::accepts_driver_state(state)
    }

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output;
}

#[derive(Resource)]
struct RegisteredDriverPort<R: __RuntimeDriverPort> {
    apply: fn(&mut World, R::Input) -> Result<R::Output, (RuntimeWorldAccessError, R::Input)>,
}

/// Short-lived command application while a runtime candidate is still unpublished.
///
/// This scope exists for product Hosts that must materialize validated project state before
/// startup. The candidate revalidates its sticky fault bridge after the scope returns, so replacing
/// protected runtime authority cannot silently publish a healthy runtime.
#[doc(hidden)]
pub struct RuntimeCandidateScope<'world> {
    world: &'world mut World,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWorldAccessError {
    #[error("runtime-managed ECS type {type_name} is protected from scoped mutation")]
    ProtectedType { type_name: &'static str },
    #[error("runtime scoped resource {type_name} is missing")]
    MissingResource { type_name: &'static str },
    #[error("runtime driver port {type_name} is unavailable while runtime state is {state:?}")]
    PortUnavailable {
        type_name: &'static str,
        state: RuntimeState,
    },
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
    fn validate_port_input<R: __RuntimeDriverPort>(
        &self,
        input: &R::Input,
    ) -> Result<(), RuntimeWorldAccessError> {
        if R::accepts_driver_input(self.state, input) {
            Ok(())
        } else {
            Err(RuntimeWorldAccessError::PortUnavailable {
                type_name: type_name::<R>(),
                state: self.state,
            })
        }
    }

    #[doc(hidden)]
    pub fn __apply_port<R: __RuntimeDriverPort>(
        &mut self,
        input: R::Input,
    ) -> Result<R::Output, RuntimeWorldAccessError> {
        self.__apply_owned_port::<R>(input)
            .map_err(|(error, _)| error)
    }

    /// Applies one typed port operation while preserving an owned input on admission failure.
    ///
    /// This is reserved for move-only Host authorities that must never be dropped merely because
    /// the Runtime state changed or the expected first-party port is unavailable.
    #[doc(hidden)]
    pub fn __apply_owned_port<R: __RuntimeDriverPort>(
        &mut self,
        input: R::Input,
    ) -> Result<R::Output, (RuntimeWorldAccessError, R::Input)> {
        if let Err(error) = self.validate_port_input::<R>(&input) {
            return Err((error, input));
        }
        if !self.world.contains_resource::<R>() {
            return Err((
                RuntimeWorldAccessError::MissingResource {
                    type_name: type_name::<R>(),
                },
                input,
            ));
        }
        if let Some(registered) = self.world.get_resource::<RegisteredDriverPort<R>>() {
            let apply = registered.apply;
            apply(self.world, input)
        } else {
            Ok(self.world.resource_mut::<R>().apply_driver_input(input))
        }
    }
}

impl App {
    /// Configures one first-party, resource-typed operation that coordinates runtime resources.
    ///
    /// Registration occurs before configuration is frozen. Runtime callers can only select this
    /// configured operation or a resource-local trait method; they cannot supply a World callback.
    /// The operation returns owned input on rejection so move-only Host responsibilities survive.
    #[doc(hidden)]
    pub fn __register_driver_port<R: __RuntimeDriverPort>(
        &mut self,
        apply: fn(&mut World, R::Input) -> Result<R::Output, (RuntimeWorldAccessError, R::Input)>,
    ) -> Result<&mut Self, crate::PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        if self
            .owner
            .world
            .contains_resource::<RegisteredDriverPort<R>>()
        {
            return Err(crate::PluginError::DuplicateDriverPort {
                type_name: type_name::<R>(),
            });
        }
        self.owner
            .world
            .insert_resource(RegisteredDriverPort::<R> { apply });
        Ok(self)
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

/// Owns waitable cleanup driven on the thread that constructs the runtime's App.
///
/// The participant may own Send worker state; its callbacks remain on the runtime owner thread.
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

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum RuntimeAdmissionReservationError {
    #[error("managed runtime fault-route capacity is exhausted")]
    CapacityExhausted,
}

/// Move-only capacity reservation for one managed runtime fault route.
///
/// Acquire this value before transferring a sealed App or close obligations. Once admission starts,
/// every failure retains the route together with the runtime owner until retirement completes.
/// This is a first-party Host bridge; ordinary callers use [`RuntimeInstance::start`].
#[doc(hidden)]
#[must_use = "a runtime admission reservation must be admitted or released"]
pub struct RuntimeAdmissionReservation {
    route: Option<FaultRouteReservation>,
}

impl Debug for RuntimeAdmissionReservation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeAdmissionReservation")
            .field("active", &self.route.is_some())
            .finish_non_exhaustive()
    }
}

impl RuntimeAdmissionReservation {
    pub fn try_acquire() -> Result<Self, RuntimeAdmissionReservationError> {
        let route = FaultRouteReservation::try_acquire()
            .ok_or(RuntimeAdmissionReservationError::CapacityExhausted)?;
        Ok(Self { route: Some(route) })
    }

    pub fn admit(
        mut self,
        sealed: SealedApp,
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Result<RuntimeCandidate, RuntimeAdmissionFailure> {
        RuntimeCandidate::admit_reserved(self.take_route(), sealed, obligations, close_policy)
    }

    fn take_route(&mut self) -> FaultRouteReservation {
        self.route
            .take()
            .expect("runtime admission reservation transfers its route once")
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum RuntimeAdmissionError {
    #[error("a managed runtime requires a sealed ready app, got {state:?}")]
    AppNotReady { state: PluginLifecycleState },
    #[error("runtime generations are exhausted")]
    GenerationExhausted,
    #[error(transparent)]
    Obligation(#[from] RuntimeObligationLedgerError),
}

#[doc(hidden)]
pub struct RuntimeAdmissionFailure {
    error: RuntimeAdmissionError,
    owner: Option<RuntimeOwner>,
}

impl Debug for RuntimeAdmissionFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeAdmissionFailure")
            .field("error", &self.error)
            .field(
                "retirement_state",
                &self.owner.as_ref().map(|owner| owner.close_state()),
            )
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
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner
            .as_ref()
            .and_then(|owner| owner.plugin_failure_report.as_ref())
    }

    fn new(error: RuntimeAdmissionError, owner: RuntimeOwner) -> Self {
        Self {
            error,
            owner: Some(owner),
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

    fn take_owner(&mut self) -> RuntimeOwner {
        self.owner
            .take()
            .expect("admission failure transfers its runtime owner once")
    }
}

impl Drop for RuntimeAdmissionFailure {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            drop(owner);
        }
    }
}

#[doc(hidden)]
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
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner.plugin_failure_report.as_ref()
    }

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

/// Unpublished execution owner used by first-party Hosts during content admission.
#[doc(hidden)]
pub struct RuntimeCandidate {
    owner: RuntimeOwner,
    generation: RuntimeGeneration,
}

impl Debug for RuntimeCandidate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeCandidate")
            .field("generation", &self.generation)
            .field("started", &self.owner.started)
            .finish_non_exhaustive()
    }
}

impl RuntimeCandidate {
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner.plugin_failure_report.as_ref()
    }

    fn admit_reserved(
        reservation: FaultRouteReservation,
        sealed: SealedApp,
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Result<Self, RuntimeAdmissionFailure> {
        let app = sealed.app;
        let mut owner = app.owner;
        owner.close_policy = close_policy;
        let obligation_error = owner.prepend_host_obligations(obligations).err();
        let reporter = owner.reporter().clone();
        owner.fault_route = Some(reservation.activate(reporter));
        let admission_error = if owner.plugin_lifecycle != PluginLifecycleState::Ready {
            Some(RuntimeAdmissionError::AppNotReady {
                state: owner.plugin_lifecycle,
            })
        } else {
            obligation_error.map(Into::into)
        };
        if let Some(error) = admission_error {
            return Err(RuntimeAdmissionFailure::new(error, owner));
        }
        let route_handler = owner
            .fault_route
            .as_ref()
            .expect("an admitted runtime owner retains its activated fault route")
            .handler();
        owner.bind_managed_runtime_error_handler(route_handler);
        let generation = match allocate_generation() {
            Ok(generation) => generation,
            Err(error) => return Err(RuntimeAdmissionFailure::new(error, owner)),
        };
        owner.managed_runtime_generation = Some(generation);
        owner.world.insert_resource(generation);

        Ok(Self { owner, generation })
    }

    #[must_use]
    pub const fn generation(&self) -> RuntimeGeneration {
        self.generation
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.owner.world
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
            .capture_driver_scope(DriverScopeAuthorityPolicy::Full, |world| {
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
        let fault_route = self.owner.fault_route_token();
        let startup =
            with_runtime_system_fault_capture(fault_route, || self.owner.complete_startup_once());
        let fault = match startup {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(RuntimeFault::app(error)),
            Err(fault) => Some(fault),
        }
        .or_else(|| reporter.fault())
        .or_else(|| {
            self.owner
                .prepare_managed_runtime()
                .err()
                .map(RuntimeFault::app)
        })
        .or_else(|| self.owner.validate_managed_runtime_authority().err())
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
    pub(crate) fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner
            .as_ref()
            .and_then(|owner| owner.plugin_failure_report.as_ref())
    }

    pub(crate) fn complete() -> Self {
        Self { owner: None }
    }

    pub(crate) fn from_app(app: App, close_policy: RuntimeClosePolicy) -> Self {
        let mut owner = app.owner;
        owner.close_policy = close_policy;
        owner.begin_close();
        Self { owner: Some(owner) }
    }

    pub(crate) fn from_reserved_app(
        app: App,
        mut reservation: RuntimeAdmissionReservation,
        close_policy: RuntimeClosePolicy,
    ) -> Self {
        let mut owner = app.owner;
        owner.close_policy = close_policy;
        owner.fault_route = Some(reservation.take_route().activate(owner.reporter().clone()));
        let route_handler = owner
            .fault_route
            .as_ref()
            .expect("a reserved preparation owner retains its activated fault route")
            .handler();
        owner.bind_managed_runtime_error_handler(route_handler);
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

#[doc(hidden)]
pub struct ReadyRuntimeCandidate {
    owner: RuntimeOwner,
    generation: RuntimeGeneration,
}

/// Host-owned destination for one atomically published runtime owner.
///
/// The contained runtime cannot be replaced through this API. A product Host reserves the slot,
/// publishes one ready candidate into it, then drives or retires that same owner.
#[doc(hidden)]
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
#[doc(hidden)]
pub enum RuntimePublicationError {
    DestinationOccupied,
    CandidateFault(RuntimeFault),
    InvalidCandidatePhase,
}

/// Retryable owner retained when atomic publication rejects a ready candidate.
#[doc(hidden)]
#[must_use = "failed runtime publication retains close authority until cleanup completes"]
pub struct RuntimePublicationFailure {
    error: RuntimePublicationError,
    retirement: RuntimeRetirement,
}

impl RuntimePublicationFailure {
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.retirement.plugin_failure_report()
    }

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
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner.plugin_failure_report.as_ref()
    }

    pub fn begin_retirement(mut self) -> RuntimeRetirement {
        self.owner.begin_close();
        RuntimeRetirement::new(self.owner, self.generation)
    }

    #[must_use]
    pub fn promote(self) -> RuntimeInstance {
        let fault = self
            .owner
            .validate_managed_runtime_authority()
            .err()
            .or_else(|| self.owner.reporter().fault())
            .map(|fault| self.owner.reporter().record_canonical(fault))
            .or_else(|| self.owner.reporter().publish());
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
        if let Err(error) = owner.validate_managed_runtime_authority() {
            let fault = owner.reporter().record_canonical(error);
            return Err(RuntimePublicationFailure::faulted(owner, generation, fault));
        }
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

#[doc(hidden)]
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
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.retirement.plugin_failure_report()
    }

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

impl Display for RuntimeRetirement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "runtime physical close is {:?}; causes={:?}",
            self.retirement_state(),
            self.close_evidence().causes(),
        )
    }
}

impl std::error::Error for RuntimeRetirement {}

impl RuntimeRetirement {
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner.plugin_failure_report.as_ref()
    }

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
    SimulationUnavailable,
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
    SimulationUnavailable,
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

/// Rejection or managed fault from one explicit custom-schedule execution.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeScheduleError {
    #[error("custom schedule execution is unavailable while runtime state is {state:?}")]
    Unavailable { state: RuntimeState },
    #[error("custom schedule execution is unavailable while a runtime control is pending")]
    ControlPending,
    #[error("built-in schedule {schedule:?} is driven exclusively by the runtime lifecycle")]
    BuiltIn { schedule: InternedScheduleLabel },
    #[error("custom schedule {schedule:?} is not installed")]
    Missing { schedule: InternedScheduleLabel },
    #[error("custom schedule execution faulted")]
    Faulted { fault: RuntimeFault },
}

impl RuntimeScheduleError {
    #[must_use]
    pub const fn fault(&self) -> Option<&RuntimeFault> {
        match self {
            Self::Faulted { fault } => Some(fault),
            Self::Unavailable { .. }
            | Self::ControlPending
            | Self::BuiltIn { .. }
            | Self::Missing { .. } => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeStartError {
    #[error("runtime capacity reservation failed: {0}")]
    Reservation(#[source] RuntimeAdmissionReservationError),
    #[error("runtime configuration failed: {0}")]
    Preparation(#[source] crate::PluginError),
    #[error("runtime admission failed: {0}")]
    Admission(#[source] RuntimeAdmissionError),
    #[error("runtime startup failed: {0:?}")]
    Startup(RuntimeFault),
}

/// A failed ordinary launch retains the same runtime owner until physical close completes.
#[must_use = "failed runtime startup retains retryable close ownership"]
pub struct RuntimeStartFailure {
    error: Box<RuntimeStartError>,
    retirement: Box<RuntimePreparationRetirement>,
}

impl Debug for RuntimeStartFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeStartFailure")
            .field("error", &self.error)
            .field("retirement", &self.retirement_state())
            .finish_non_exhaustive()
    }
}

impl Display for RuntimeStartFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for RuntimeStartFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error.as_ref())
    }
}

impl RuntimeStartFailure {
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.retirement.plugin_failure_report()
    }

    fn new(error: RuntimeStartError, retirement: RuntimePreparationRetirement) -> Self {
        Self {
            error: Box::new(error),
            retirement: Box::new(retirement),
        }
    }

    fn from_owner(error: RuntimeStartError, mut owner: RuntimeOwner) -> Self {
        owner.begin_close();
        Self::new(error, RuntimePreparationRetirement { owner: Some(owner) })
    }

    #[must_use]
    pub fn error(&self) -> &RuntimeStartError {
        &self.error
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.retirement
            .close_evidence()
            .expect("ordinary startup failures always retain their runtime owner")
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        self.retirement.drive_retirement()
    }
}

/// Owns managed execution and retirement on the thread that constructs its App.
///
/// Transfer a replayable definition to another thread and construct a fresh App there instead of
/// moving an existing runtime or its retirement owner.
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
    /// Borrows the existing lifecycle report, including terminal shutdown failures.
    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner.plugin_failure_report.as_ref()
    }

    /// Starts a configured application using the engine's managed execution lifecycle.
    ///
    /// Failure blocks for one finite physical-close attempt before returning its owner. If close
    /// remains incomplete, the caller retains the failure and explicitly drives retirement again.
    pub fn start(app: App) -> Result<Self, RuntimeStartFailure> {
        Self::start_with_options(
            app,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
    }

    /// Starts an embedded runtime with Host-owned close obligations and a finite close policy.
    ///
    /// A failed launch blocks for one close attempt within that policy. Responsive UI hosts should
    /// use staged admission and nonblocking retirement instead.
    pub fn start_with_options(
        app: App,
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Result<Self, RuntimeStartFailure> {
        Self::start_inner(app, obligations, close_policy).map_err(|mut failure| {
            failure
                .retirement
                .owner
                .as_mut()
                .expect("ordinary startup failure retains its runtime owner")
                .finish_close_attempt();
            failure
        })
    }

    fn start_inner(
        mut app: App,
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> Result<Self, RuntimeStartFailure> {
        app.owner.close_policy = close_policy;
        if let Err(error) = app.owner.prepend_host_obligations(obligations) {
            return Err(RuntimeStartFailure::from_owner(
                RuntimeStartError::Admission(error.into()),
                app.owner,
            ));
        }
        let reservation = match RuntimeAdmissionReservation::try_acquire() {
            Ok(reservation) => reservation,
            Err(error) => {
                return Err(RuntimeStartFailure::new(
                    RuntimeStartError::Reservation(error),
                    RuntimePreparationRetirement::from_app(app, close_policy),
                ));
            }
        };
        if let Err(error) = app.seal_internal() {
            return Err(RuntimeStartFailure::new(
                RuntimeStartError::Preparation(error),
                RuntimePreparationRetirement::from_reserved_app(app, reservation, close_policy),
            ));
        }
        let candidate = match reservation.admit(
            SealedApp { app },
            RuntimeObligationLedger::new(),
            close_policy,
        ) {
            Ok(candidate) => candidate,
            Err(mut failure) => {
                let error = RuntimeStartError::Admission(failure.error());
                return Err(RuntimeStartFailure::from_owner(error, failure.take_owner()));
            }
        };
        match candidate.complete_startup() {
            Ok(ready) => Ok(ready.promote()),
            Err(failure) => {
                let error = RuntimeStartError::Startup(failure.fault().clone());
                let RuntimeRetirement { owner, .. } = *failure.retirement;
                Err(RuntimeStartFailure::from_owner(error, owner))
            }
        }
    }

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

    /// Blocks for one physical-close attempt within this runtime's existing close policy.
    ///
    /// This consumes a finite headless or one-shot runtime. An unfinished attempt returns the same
    /// retirement owner for explicit retry; it never loops through automatic retries. UI hosts
    /// should use [`Self::request_control`] and [`Self::drive`] to remain responsive.
    /// A terminal teardown failure also returns that owner with `Retired` state and retained
    /// failure evidence. Historical timeouts do not turn a subsequently completed close into failure.
    pub fn finish(self) -> Result<RuntimeCloseEvidence, RuntimeRetirement> {
        let mut retirement = self.begin_retirement();
        retirement.owner.finish_close_attempt();
        match retirement.retirement_state() {
            RuntimeCandidateRetirementState::Retired
                if !retirement.close_evidence().has_terminal_failure() =>
            {
                Ok(retirement.close_evidence().clone())
            }
            RuntimeCandidateRetirementState::Retired
            | RuntimeCandidateRetirementState::Retiring
            | RuntimeCandidateRetirementState::RetirementIncomplete => Err(retirement),
        }
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.owner.world
    }

    /// Executes one installed custom schedule between managed drives.
    ///
    /// The runtime must be Running with no pending control. Built-in startup, frame and cleanup
    /// schedules remain exclusive to the automatic lifecycle. Execution uses the same authority,
    /// deferred-command and sticky-fault boundaries as an automatic schedule, without advancing
    /// frame/tick clocks, clearing change trackers or adding this schedule to automatic frame order.
    /// A custom schedule retains its configured deferred-command policy.
    ///
    /// This exclusive owner borrow prevents reentrant execution. Systems cannot obtain the owner
    /// through their World, and this entry exposes neither a mutable World nor a driver callback.
    ///
    /// # Errors
    ///
    /// Returns a rejection before systems execute for an unavailable state, pending control,
    /// built-in label or absent custom schedule. A managed execution fault is retained by the
    /// runtime and returned as [`RuntimeScheduleError::Faulted`]; ordinary Stop/Close remains
    /// available through [`Self::request_control`] and [`Self::drive`].
    pub fn run_schedule(
        &mut self,
        schedule: impl ScheduleLabel,
    ) -> Result<(), RuntimeScheduleError> {
        let state = self.state();
        if state != RuntimeState::Running {
            return Err(RuntimeScheduleError::Unavailable { state });
        }
        if self.pending_control.is_some() {
            return Err(RuntimeScheduleError::ControlPending);
        }
        let schedule = schedule.intern();
        if crate::is_built_in_schedule(schedule) {
            return Err(RuntimeScheduleError::BuiltIn { schedule });
        }
        if !self.owner.schedules.contains(schedule) {
            return Err(RuntimeScheduleError::Missing { schedule });
        }

        let fault_route = self.owner.fault_route_token();
        let result = with_runtime_system_fault_capture(fault_route, || {
            self.owner.run_managed_schedule(schedule)
        });
        let fault = match result {
            Ok(Ok(())) => self.owner.reporter().fault(),
            Ok(Err(error)) => Some(RuntimeFault::app(error)),
            Err(fault) => Some(fault),
        };
        if let Some(fault) = fault {
            let RuntimeDriveError::Faulted { fault } = self.enter_fault(fault);
            return Err(RuntimeScheduleError::Faulted { fault });
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn world_mut_for_tests(&mut self) -> &mut World {
        &mut self.owner.world
    }

    /// Grants a platform or Host one short-lived managed runtime scope.
    ///
    /// The scope admits only typed, resource-local driver ports and applies each port's runtime
    /// state gate. It exposes neither the `World` nor generic resource access. A runtime carrying
    /// a prior fault still permits ports that explicitly accept its current state so the driver can
    /// retire surfaces and other owned services; the existing fault remains sticky and observable.
    /// This fault-retirement path deliberately does not revalidate the general runtime authority:
    /// that authority may be the reason the runtime was isolated, while the typed port remains the
    /// only operation available to release its owned native state. The
    /// hidden cross-crate carrier is pre-1.0 plumbing, not the shared Adapter contract tracked by
    /// OQ-038.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeScopeError::Unavailable`] while stepping or after the runtime is stopped.
    /// Returns [`RuntimeScopeError::Faulted`] when healthy-scope authority was replaced or the
    /// operation reports a new canonical runtime fault.
    #[doc(hidden)]
    pub fn with_driver_scope<R>(
        &mut self,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> R,
    ) -> Result<R, RuntimeScopeError> {
        let state = self.state();
        if matches!(state, RuntimeState::Stepping | RuntimeState::Stopped) {
            return Err(RuntimeScopeError::Unavailable { state });
        }
        let authority_policy = if self.fault().is_some()
            && matches!(
                state,
                RuntimeState::Faulted | RuntimeState::Stopping | RuntimeState::CloseIncomplete
            ) {
            DriverScopeAuthorityPolicy::FaultRetirement
        } else {
            DriverScopeAuthorityPolicy::Full
        };
        self.owner
            .capture_driver_scope(authority_policy, |world| {
                let mut scope = RuntimeDriverScope { world, state };
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

        if control == RuntimeControl::StepFixedTick && !self.owner.simulation_is_ready() {
            return RuntimeControlRequestResult::Rejected(
                RuntimeControlRejection::SimulationUnavailable,
            );
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
                    if !self.owner.simulation_is_ready() {
                        self.set_control_status(
                            pending.ticket,
                            RuntimeControlStatus::Failed(
                                RuntimeControlFailure::SimulationUnavailable,
                            ),
                        );
                        let frame = self.drive_app_frame(real_delta, RuntimeFrameMode::Paused)?;
                        return Ok(RuntimeDriveOutcome {
                            state: self.state,
                            frame: Some(frame),
                        });
                    }
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
        let Some(mut settings) = self.owner.world.get_resource_mut::<RuntimeTimeSettings>() else {
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
        let fault_route = self.owner.fault_route_token();
        let result = with_runtime_system_fault_capture(fault_route, || match mode {
            RuntimeFrameMode::Running => self.owner.run_managed_frame(real_delta),
            RuntimeFrameMode::Paused => self.owner.run_paused_frame(real_delta),
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
        let fault_route = self.owner.fault_route_token();
        let result = with_runtime_system_fault_capture(fault_route, || {
            self.owner.run_exact_fixed_tick(real_delta)
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
            RuntimeState::Stopped if self.owner.close_evidence().has_terminal_failure() => {
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

fn allocate_generation() -> Result<RuntimeGeneration, RuntimeAdmissionError> {
    NEXT_RUNTIME_GENERATION
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map(RuntimeGeneration)
        .map_err(|_| RuntimeAdmissionError::GenerationExhausted)
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

fn with_runtime_system_fault_capture<R>(
    fault_route: Option<FaultRouteToken>,
    operation: impl FnOnce() -> R,
) -> Result<R, RuntimeFault> {
    let _execution = fault_route.map(FaultRouteToken::enter).transpose()?;
    Ok(operation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::{Event, On};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize},
        mpsc,
    };

    struct TerminalShutdownFailure;

    struct StartupFailure;

    impl crate::Plugin for StartupFailure {
        fn declaration() -> &'static crate::PluginDeclaration {
            static DECLARATION: crate::PluginDeclaration = crate::PluginDeclaration::new(
                crate::PluginId::new("nara.test.startup-failure"),
                crate::PluginCategory::Runtime,
            );
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), crate::PluginError> {
            Err(crate::PluginError::SetupFailed {
                plugin: Self::declaration().id,
                message: "injected startup failure".to_owned(),
            })
        }
    }

    #[test]
    fn startup_failure_does_not_report_successful_shutdown_as_failed() {
        let mut app = App::new();
        assert!(app.add_plugin(StartupFailure).is_err());
        let failure = RuntimeInstance::start(app).unwrap_err();
        let state = failure.retirement_state();
        let shutdown_failed = failure.close_evidence().plugin_shutdown_failed();
        drop(failure);

        assert_eq!(state, RuntimeCandidateRetirementState::Retired);
        assert!(!shutdown_failed);
    }

    impl crate::Plugin for TerminalShutdownFailure {
        fn declaration() -> &'static crate::PluginDeclaration {
            static DECLARATION: crate::PluginDeclaration = crate::PluginDeclaration::new(
                crate::PluginId::new("nara.test.terminal-shutdown-failure"),
                crate::PluginCategory::Runtime,
            );
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), crate::PluginError> {
            Ok(())
        }

        fn shutdown(
            &self,
            _context: &mut crate::PluginShutdownContext<'_>,
        ) -> Result<(), crate::PluginError> {
            Err(crate::PluginError::SetupFailed {
                plugin: Self::declaration().id,
                message: "injected terminal shutdown failure".to_owned(),
            })
        }
    }

    #[test]
    fn finish_reports_terminal_plugin_shutdown_failure_after_physical_close() {
        let mut app = App::new();
        app.add_plugin(TerminalShutdownFailure).unwrap();

        let failure = RuntimeInstance::start(app).unwrap().finish().unwrap_err();

        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::Retired
        );
        assert!(failure.close_evidence().plugin_shutdown_failed());
    }

    struct CountedHostClose {
        begins: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for CountedHostClose {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            Ok(RuntimeCloseProgress::Complete)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            unreachable!("this participant completes at begin-close")
        }
    }

    impl Drop for CountedHostClose {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[derive(Event)]
    struct ObserverInitializationProbe;

    #[derive(Resource)]
    struct UnconfiguredDriverPort;

    impl __RuntimeDriverPort for UnconfiguredDriverPort {
        type Input = Arc<AtomicBool>;
        type Output = ();

        fn apply_driver_input(&mut self, _input: Self::Input) {}
    }

    fn mark_configured_driver_call(
        _world: &mut World,
        called: Arc<AtomicBool>,
    ) -> Result<(), (RuntimeWorldAccessError, Arc<AtomicBool>)> {
        called.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[test]
    fn missing_driver_resource_cannot_run_a_configured_world_operation() {
        let mut app = App::new();
        app.__register_driver_port::<UnconfiguredDriverPort>(mark_configured_driver_call)
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let called = Arc::new(AtomicBool::new(false));

        let result = runtime
            .with_driver_scope(|scope| scope.__apply_port::<UnconfiguredDriverPort>(called.clone()))
            .unwrap();

        assert_eq!(
            result,
            Err(RuntimeWorldAccessError::MissingResource {
                type_name: type_name::<UnconfiguredDriverPort>(),
            })
        );
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn installed_driver_resource_does_not_select_an_unconfigured_world_operation() {
        let mut app = App::new();
        app.insert_resource(UnconfiguredDriverPort).unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let called = Arc::new(AtomicBool::new(false));

        runtime
            .with_driver_scope(|scope| scope.__apply_port::<UnconfiguredDriverPort>(called.clone()))
            .unwrap()
            .unwrap();

        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn configured_driver_operation_is_unique_and_frozen_before_execution() {
        let mut app = App::new();
        app.insert_resource(UnconfiguredDriverPort).unwrap();
        app.__register_driver_port::<UnconfiguredDriverPort>(mark_configured_driver_call)
            .unwrap();
        assert!(matches!(
            app.__register_driver_port::<UnconfiguredDriverPort>(mark_configured_driver_call),
            Err(crate::PluginError::DuplicateDriverPort { .. })
        ));
        app.seal_internal().unwrap();
        assert!(matches!(
            app.__register_driver_port::<UnconfiguredDriverPort>(mark_configured_driver_call),
            Err(crate::PluginError::AppSealed)
        ));
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let called = Arc::new(AtomicBool::new(false));

        runtime
            .with_driver_scope(|scope| scope.__apply_port::<UnconfiguredDriverPort>(called.clone()))
            .unwrap()
            .unwrap();

        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn configured_driver_rejection_returns_the_original_owned_input() {
        fn reject(
            _world: &mut World,
            input: Arc<AtomicBool>,
        ) -> Result<(), (RuntimeWorldAccessError, Arc<AtomicBool>)> {
            Err((
                RuntimeWorldAccessError::MissingResource {
                    type_name: "fixture",
                },
                input,
            ))
        }
        let mut app = App::new();
        app.insert_resource(UnconfiguredDriverPort).unwrap();
        app.__register_driver_port::<UnconfiguredDriverPort>(reject)
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let input = Arc::new(AtomicBool::new(false));

        let (_, returned) = runtime
            .with_driver_scope(|scope| {
                scope.__apply_owned_port::<UnconfiguredDriverPort>(input.clone())
            })
            .unwrap()
            .unwrap_err();

        assert!(Arc::ptr_eq(&input, &returned));
    }

    #[test]
    fn admission_observer_initialization_unwind_retains_host_close_obligations() {
        let begins = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.observer-admission-host-close"),
                CountedHostClose {
                    begins: begins.clone(),
                    drops: drops.clone(),
                },
            )
            .unwrap();
        let mut app = App::new();
        app.world_mut().unwrap().add_observer(
            |_: On<nara_ecs::lifecycle::Add<nara_ecs::observer::Observer>>| {
                panic!("injected pending observer initialization panic");
            },
        );
        app.add_observer(|_: On<ObserverInitializationProbe>| {})
            .unwrap();
        let sealed = app.seal().unwrap();
        let reservation = RuntimeAdmissionReservation::try_acquire().unwrap();

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            reservation.admit(sealed, obligations, RuntimeClosePolicy::default())
        }));
        let begins_before_retirement = begins.load(Ordering::SeqCst);
        let drops_before_retirement = drops.load(Ordering::SeqCst);
        let quarantine = drive_runtime_quarantine();

        assert!(outcome.is_err());
        assert_eq!(begins_before_retirement, 0);
        assert_eq!(drops_before_retirement, 0);
        assert_eq!(quarantine.current_thread_retained(), 0);
        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    struct WorkerClose {
        release: Option<mpsc::Sender<()>>,
        worker: Option<std::thread::JoinHandle<()>>,
        joined: Arc<AtomicBool>,
    }

    impl RuntimeCloseParticipant for WorkerClose {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.release.take().unwrap().send(()).unwrap();
            Ok(RuntimeCloseProgress::Pending)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            if self.worker.as_ref().unwrap().is_finished() {
                self.worker.take().unwrap().join().unwrap();
                self.joined.store(true, Ordering::SeqCst);
                Ok(RuntimeCloseProgress::Complete)
            } else {
                Ok(RuntimeCloseProgress::Pending)
            }
        }
    }

    #[test]
    fn normal_configuration_drop_joins_its_worker_before_returning() {
        let (release, receiver) = mpsc::channel();
        let joined = Arc::new(AtomicBool::new(false));
        let worker = std::thread::spawn(move || {
            receiver.recv().unwrap();
            std::thread::sleep(Duration::from_millis(5));
        });
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.normal-drop-worker"),
                WorkerClose {
                    release: Some(release),
                    worker: Some(worker),
                    joined: joined.clone(),
                },
            )
            .unwrap();
        let app = App::new_with_runtime_obligations(obligations);

        drop(app);

        assert!(joined.load(Ordering::SeqCst));
        assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
    }

    #[test]
    fn managed_scope_maintenance_cannot_hide_restored_fault_bridge_mutation() {
        fn ignored_handler(_: BevyError, _: nara_ecs::error::ErrorContext) {}

        for mutation in 0..2 {
            let mut runtime = RuntimeInstance::start(App::new()).unwrap();
            let reporter = runtime.fault_reporter();
            let failure = runtime
                .owner
                .capture_driver_scope(DriverScopeAuthorityPolicy::Full, |world| {
                    run_runtime_fault_bridge_maintenance(world, |world| {
                        world.increment_change_tick();
                        match mutation {
                            0 => {
                                let mut reporter = world.resource_mut::<RuntimeFaultReporter>();
                                let original = reporter.clone();
                                *reporter = RuntimeFaultReporter::new();
                                *reporter = original;
                            }
                            1 => {
                                let mut handler = world.resource_mut::<FallbackErrorHandler>();
                                let original = *handler;
                                *handler = FallbackErrorHandler(ignored_handler);
                                *handler = original;
                            }
                            _ => unreachable!(),
                        }
                    })
                })
                .unwrap_err();

            assert_eq!(failure.kind(), RuntimeFaultKind::FaultReporterAuthority);
            assert_eq!(
                reporter.fault().unwrap().kind(),
                RuntimeFaultKind::FaultReporterAuthority
            );
            assert_eq!(
                runtime.drive(Duration::ZERO).unwrap_err().fault().kind(),
                RuntimeFaultKind::FaultReporterAuthority
            );
        }
    }
}
