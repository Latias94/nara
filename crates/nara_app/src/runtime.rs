use std::{
    any::{TypeId, type_name},
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
    fmt::{self, Debug, Display, Formatter},
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use bevy_ecs::error::{BevyError, ErrorHandler, FallbackErrorHandler};
use nara_ecs::{
    Command, Mut, Resource, World,
    change_detection::Tick,
    component::Mutable,
    lifecycle::HookContext,
    observer::Observer,
    schedule::{IntoScheduleConfigs, Schedule, Schedules},
    world::DeferredWorld,
};
use thiserror::Error;

use crate::{
    App, AppExitRequests, AppFrameOutcome, AppRunError, CoreStage, FixedTime, FixedUpdateSet,
    InstalledPlugin, InstalledRuntimeAuthorityValidator, PluginFailureReport, PluginHook, PluginId,
    PluginLifecycleState, PluginShutdownError, RealTime, RenderTime, RuntimeFrameStatus,
    RuntimeTimeSettings, SealedApp, StartupStage, VirtualTime,
};

mod fault_route;

use fault_route::{FaultRouteLease, FaultRouteReservation, FaultRouteToken};

const MAX_RETAINED_CONTROL_RESULTS: usize = 32;
const MAX_QUARANTINED_RUNTIME_OWNERS_PER_THREAD: usize = 32;
const MAX_QUARANTINED_RUNTIME_OWNERS_PER_PROCESS: usize = 128;
const DEFAULT_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(1);
static QUARANTINED_RUNTIME_OWNER_COUNT: AtomicUsize = AtomicUsize::new(0);
static TOTAL_QUARANTINED_RUNTIME_OWNERS: AtomicU64 = AtomicU64::new(0);
static TOTAL_REAPED_RUNTIME_OWNERS: AtomicU64 = AtomicU64::new(0);

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
    pub const MAX_CODE_BYTES: usize = 96;
    /// The maximum encoded byte length accepted for a diagnostic summary.
    pub const MAX_SUMMARY_BYTES: usize = 1_024;
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
        if !is_valid_runtime_fault_identifier(code, Self::MAX_CODE_BYTES, false) {
            return Err(RuntimeFaultDetailError::InvalidCode);
        }
        if !is_valid_runtime_fault_summary(summary) {
            return Err(RuntimeFaultDetailError::InvalidSummary);
        }
        if !is_valid_runtime_fault_identifier(origin, Self::MAX_ORIGIN_BYTES, true) {
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

fn is_valid_runtime_fault_identifier(
    value: &str,
    maximum_bytes: usize,
    reject_sensitive_shape: bool,
) -> bool {
    let Some(first) = value.bytes().next() else {
        return false;
    };
    value.len() <= maximum_bytes
        && value.is_ascii()
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && (!reject_sensitive_shape || !contains_runtime_fault_sensitive_shape(value))
}

fn is_valid_runtime_fault_summary(value: &'static str) -> bool {
    !value.is_empty()
        && value.len() <= RuntimeFaultDetail::MAX_SUMMARY_BYTES
        && !value.chars().any(char::is_control)
        && !value.chars().any(is_runtime_fault_unsafe_format_character)
        && !contains_runtime_fault_sensitive_shape(value)
        && !resembles_runtime_fault_absolute_path(value)
}

fn contains_runtime_fault_sensitive_shape(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    lowercase.contains("bearer")
        || lowercase.contains("token")
        || lowercase.contains("password")
        || lowercase.contains("credential")
        || lowercase.contains("secret")
        || lowercase.contains("api_key")
        || lowercase.contains("api-key")
        || lowercase.contains("://")
}

fn resembles_runtime_fault_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with("\\\\")
        || value.starts_with("//")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

fn is_runtime_fault_unsafe_format_character(character: char) -> bool {
    matches!(
        character,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180b}'..='\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e007f}'
    )
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
    /// A terminal plugin-shutdown failure also returns that owner with `Retired` state and retained
    /// failure evidence. Historical timeouts do not turn a subsequently completed close into failure.
    pub fn finish(self) -> Result<RuntimeCloseEvidence, RuntimeRetirement> {
        let mut retirement = self.begin_retirement();
        retirement.owner.finish_close_attempt();
        match retirement.retirement_state() {
            RuntimeCandidateRetirementState::Retired
                if !retirement.close_evidence().plugin_shutdown_failed() =>
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

pub(super) struct RuntimeOwner {
    state: Option<RuntimeOwnerState>,
    quarantine_accounted: bool,
}

pub(super) struct RuntimeOwnerState {
    pub(super) world: World,
    pub(super) schedules: Schedules,
    pub(super) plugins: Vec<InstalledPlugin>,
    pub(super) runtime_fault_reporter: RuntimeFaultReporter,
    pub(super) managed_runtime_generation: Option<RuntimeGeneration>,
    pub(super) runtime_error_handler: Option<ErrorHandler>,
    pub(super) runtime_fault_bridge: Option<RuntimeFaultBridgeIdentity>,
    pub(super) runtime_authority_validator: Option<InstalledRuntimeAuthorityValidator>,
    pub(super) pending_observers: Vec<Observer>,
    pub(super) plugin_lifecycle: PluginLifecycleState,
    pub(super) plugin_failure_report: Option<PluginFailureReport>,
    pub(super) active_plugin_hook: Option<(PluginId, PluginHook)>,
    pub(super) started: bool,
    pub(super) obligations: RuntimeObligationLedger,
    fault_route: Option<FaultRouteLease>,
    close_policy: RuntimeClosePolicy,
    close_evidence: RuntimeCloseEvidence,
    close_state: OwnerCloseState,
}

impl RuntimeOwner {
    pub(super) fn new() -> Self {
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
    fn prepend_host_obligations(
        &mut self,
        mut obligations: RuntimeObligationLedger,
    ) -> Result<(), RuntimeObligationLedgerError> {
        let admission = obligations.preflight_append(&self.obligations);
        obligations.append_for_retirement(std::mem::take(&mut self.obligations));
        self.obligations = obligations;
        admission
    }

    #[cfg(test)]
    pub(super) fn admit_configuration_for_tests(&mut self) {
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
            pending_observers: Vec::new(),
            plugin_lifecycle: PluginLifecycleState::Configuring,
            plugin_failure_report: None,
            active_plugin_hook: None,
            started: false,
        }
    }

    fn begin_close(&mut self) {
        if matches!(self.close_state, OwnerCloseState::Open) {
            self.runtime_fault_reporter.mark_closing();
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
                self.runtime_fault_reporter.mark_closing();
                self.close_state = OwnerCloseState::Closing(OwnerCloseAttempt {
                    deadline: deadline_after(self.close_policy.timeout()),
                    plugin_shutdown,
                });
            }
            OwnerCloseState::Closing(_) | OwnerCloseState::Complete => {}
        }
    }

    fn finish_close_attempt(&mut self) {
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

    fn drive_close(&mut self) {
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

    fn reporter(&self) -> &RuntimeFaultReporter {
        &self.runtime_fault_reporter
    }

    pub(super) fn begin_fault_bridge_epoch(
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

    pub(super) fn validate_fault_bridge_epoch(
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

    fn fault_route_token(&self) -> Option<FaultRouteToken> {
        self.fault_route.as_ref().map(FaultRouteLease::token)
    }

    fn capture_driver_scope<R>(
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
    use std::sync::{atomic::AtomicBool, mpsc};

    struct TerminalShutdownFailure;

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
