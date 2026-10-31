//! Immutable capture/adapter pairing and instance-local, epoch-local native state.

use std::{
    any::Any,
    mem::size_of,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{WgpuDeviceRequirements, WgpuRenderProducerId};

use super::{
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuRenderProgramError,
};

static NEXT_FEATURE_DECLARATION: AtomicU64 = AtomicU64::new(1);
static NEXT_FEATURE_INSTANCE: AtomicU64 = AtomicU64::new(1);
const MAX_ACTIVE_NATIVE_FEATURE_STATES: usize = 1024;

/// Native resources may cross polling callbacks on native targets. Browser resources stay on
/// their JavaScript agent. This bound does not grant submission or device ownership.
#[cfg(not(target_arch = "wasm32"))]
pub trait WgpuNativeResources: Send + Sync + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + 'static> WgpuNativeResources for T {}
#[cfg(target_arch = "wasm32")]
pub trait WgpuNativeResources: 'static {}
#[cfg(target_arch = "wasm32")]
impl<T: 'static> WgpuNativeResources for T {}

/// An exact native adapter paired with one immutable CPU capture type.
///
/// Definitions are immutable and shareable. Native state belongs to one Host, producer,
/// instance and device epoch. Every prepared value owns everything its encoding needs; the
/// Host retains it through the original submission even when publication fails. Adapters are
/// trusted native Rust: they must obey declared accesses, immutable imports and Store semantics.
pub trait WgpuRenderFeature: Send + Sync + 'static {
    type Capture: Send + Sync + 'static;
    type State: WgpuNativeResources;
    type Prepared: WgpuNativeResources;
    type Report: WgpuNativeResources;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        WgpuDeviceRequirements::default()
    }

    /// Checks the exact capture before native resource allocation, state creation or
    /// upload. This context grants only the actual enabled capability facts.
    fn preflight(
        &self,
        _capture: &Self::Capture,
        _context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        Ok(())
    }

    /// Creates epoch-local state under the same reservation and private-allocation budget as
    /// preparation. Keep an allocation guard alongside every retained private native allocation.
    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError>;

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError>;

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError>;
}

struct FeatureDefinition<F: WgpuRenderFeature> {
    feature: F,
    requirements: WgpuDeviceRequirements,
}

/// A retained definition never resolves its implementation through a later global registry.
pub struct WgpuFeatureDefinition<F: WgpuRenderFeature>(Arc<FeatureDefinition<F>>);

impl<F: WgpuRenderFeature> Clone for WgpuFeatureDefinition<F> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<F: WgpuRenderFeature> std::fmt::Debug for WgpuFeatureDefinition<F> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuFeatureDefinition")
            .field("type", &std::any::type_name::<F>())
            .finish_non_exhaustive()
    }
}

impl<F: WgpuRenderFeature> WgpuFeatureDefinition<F> {
    #[must_use]
    pub fn new(feature: F) -> Self {
        let requirements = feature.device_requirements();
        Self(Arc::new(FeatureDefinition {
            feature,
            requirements,
        }))
    }

    #[must_use]
    pub fn device_requirements(&self) -> &WgpuDeviceRequirements {
        &self.0.requirements
    }

    /// Declares a new logical effect incarnation independently of any Host or producer.
    ///
    /// Each call acquires a process-local identity that is never reused. Exhausting the
    /// identity space aborts the process rather than allowing an old incarnation to alias.
    /// Clone the declaration to retain the same incarnation across immutable captures.
    #[must_use]
    pub fn declare(&self) -> WgpuFeatureDeclaration<F> {
        let id = NEXT_FEATURE_DECLARATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .unwrap_or_else(|_| std::process::abort());
        WgpuFeatureDeclaration {
            id,
            definition: self.clone(),
            lifetime: Arc::new(()),
        }
    }
}

/// An immutable original logical effect incarnation and its exact implementation.
///
/// A declaration carries no Host admission authority. Dropping one clone does not close a
/// bound Feature instance; the actual instance owners retain their explicit Drop-close
/// semantics. Retained policies and captures keep declarations usable on a replacement Host.
/// Clones name the same effect even across policy installations on one producer; independently
/// mutable effects require separate calls to [`WgpuFeatureDefinition::declare`].
pub struct WgpuFeatureDeclaration<F: WgpuRenderFeature> {
    id: u64,
    definition: WgpuFeatureDefinition<F>,
    lifetime: Arc<()>,
}

impl<F: WgpuRenderFeature> Clone for WgpuFeatureDeclaration<F> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            definition: self.definition.clone(),
            lifetime: Arc::clone(&self.lifetime),
        }
    }
}

impl<F: WgpuRenderFeature> std::fmt::Debug for WgpuFeatureDeclaration<F> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuFeatureDeclaration")
            .field("id", &self.id)
            .field("definition", &self.definition)
            .finish_non_exhaustive()
    }
}

impl<F: WgpuRenderFeature> WgpuFeatureDeclaration<F> {
    #[must_use]
    pub fn device_requirements(&self) -> &WgpuDeviceRequirements {
        self.definition.device_requirements()
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn lifetime(&self) -> Weak<()> {
        Arc::downgrade(&self.lifetime)
    }

    pub(crate) fn definition(&self) -> &WgpuFeatureDefinition<F> {
        &self.definition
    }

    pub(crate) fn binding_metadata_bytes(&self) -> Option<usize> {
        [
            size_of::<WgpuFeatureInstance<F>>(),
            size_of::<FeatureInstanceIdentity>(),
            size_of::<FeatureDefinition<F>>(),
            // Instance, identity, original definition and declaration lifetime counters.
            8 * size_of::<usize>(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
    }
}

#[derive(Debug)]
pub(crate) struct FeatureInstanceIdentity {
    pub(crate) id: u64,
    pub(crate) backend_instance_id: u64,
    pub(crate) producer: WgpuRenderProducerId,
    pub(crate) producer_closed: Arc<AtomicBool>,
    closed: AtomicBool,
}

impl FeatureInstanceIdentity {
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.producer_closed.load(Ordering::Acquire)
    }
}

/// Admission authority for one installed Feature instance. Drop irreversibly closes admission.
/// Retained CPU operations keep their original definition but cannot reopen the instance.
pub struct WgpuFeatureInstance<F: WgpuRenderFeature> {
    pub(crate) identity: Arc<FeatureInstanceIdentity>,
    definition: Arc<FeatureDefinition<F>>,
}

impl<F: WgpuRenderFeature> std::fmt::Debug for WgpuFeatureInstance<F> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuFeatureInstance")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl<F: WgpuRenderFeature> Drop for WgpuFeatureInstance<F> {
    fn drop(&mut self) {
        self.identity.closed.store(true, Ordering::Release);
    }
}

impl<F: WgpuRenderFeature> WgpuFeatureInstance<F> {
    pub(crate) fn new(
        backend_instance_id: u64,
        producer: WgpuRenderProducerId,
        producer_closed: Arc<AtomicBool>,
        definition: &WgpuFeatureDefinition<F>,
    ) -> Result<Self, WgpuRenderProgramError> {
        let id = NEXT_FEATURE_INSTANCE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| WgpuRenderProgramError::IdentityExhausted)?;
        Ok(Self {
            identity: Arc::new(FeatureInstanceIdentity {
                id,
                backend_instance_id,
                producer,
                producer_closed,
                closed: AtomicBool::new(false),
            }),
            definition: Arc::clone(&definition.0),
        })
    }

    pub(super) fn capture(&self, capture: Arc<F::Capture>) -> Arc<dyn CapturedOperation> {
        Arc::new(TypedCapture::<F> {
            identity: Arc::clone(&self.identity),
            definition: Arc::clone(&self.definition),
            capture,
        })
    }
}

trait ErasedNativeState: WgpuNativeResources {
    fn as_any(&self) -> &dyn Any;
}

struct TypedNativeState<F: WgpuRenderFeature> {
    value: Mutex<F::State>,
    // Keep the exact adapter and its cleanup dependencies until state destruction completes.
    _definition: Arc<FeatureDefinition<F>>,
}
impl<F: WgpuRenderFeature> ErasedNativeState for TypedNativeState<F> {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct NativeStateEntry {
    identity: Arc<FeatureInstanceIdentity>,
    epoch: u64,
    value: Arc<dyn ErasedNativeState>,
}

/// The Host owns mutable native caches. Submitted reservations independently retain actual use.
#[derive(Default)]
pub(crate) struct NativeFeatureStates(Vec<NativeStateEntry>);

impl NativeFeatureStates {
    pub(crate) fn maintain(&mut self, epoch: u64) {
        self.0
            .retain(|entry| entry.epoch == epoch && !entry.identity.is_closed());
    }

    pub(crate) fn discard(&mut self, identities: impl Iterator<Item = u64>) {
        for id in identities {
            self.0.retain(|entry| entry.identity.id != id);
        }
    }
}

pub(super) fn state_table_metadata_bytes() -> usize {
    size_of::<NativeFeatureStates>()
        + MAX_ACTIVE_NATIVE_FEATURE_STATES * size_of::<NativeStateEntry>()
}

/// Exact inline typed wrappers, including their Arc counters. User-owned indirect allocations
/// remain the capture/adapter's responsibility; this does not estimate their heap contents.
pub(super) fn operation_metadata_bytes<F: WgpuRenderFeature>() -> Option<usize> {
    let mut total = 0usize;
    for bytes in [
        size_of::<TypedCapture<F>>(),
        size_of::<TypedNativeState<F>>(),
        size_of::<TypedPrepared<F>>(),
        size_of::<TypedReport<F>>(),
    ] {
        total = total
            .checked_add(bytes)?
            .checked_add(2 * size_of::<usize>())?;
    }
    for bytes in [
        crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<
            Arc<dyn ErasedNativeState>,
        >(),
        crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<
            Arc<dyn PreparedOperation>,
        >(),
        crate::submission::WgpuSubmissionReservation::modeled_retain_bytes::<
            Arc<dyn OperationReport>,
        >(),
    ] {
        total = total.checked_add(bytes)?;
    }
    Some(total)
}

pub(crate) trait CapturedOperation: Send + Sync {
    fn identity(&self) -> &Arc<FeatureInstanceIdentity>;
    #[cfg(feature = "stock-ui")]
    fn capture_value(&self) -> &dyn Any;
    fn requirements(&self) -> &WgpuDeviceRequirements;
    fn preflight(
        &self,
        context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError>;
    fn prepare(
        &self,
        states: &mut NativeFeatureStates,
        epoch: u64,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Arc<dyn PreparedOperation>, WgpuRenderProgramError>;
}

pub(crate) trait PreparedOperation: WgpuNativeResources {
    fn encode(
        &self,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Arc<dyn OperationReport>, WgpuRenderProgramError>;
}

pub(crate) trait OperationReport: WgpuNativeResources {
    fn as_any(&self) -> &dyn Any;
}

struct TypedReport<F: WgpuRenderFeature> {
    value: F::Report,
    _definition: Arc<FeatureDefinition<F>>,
}
impl<F: WgpuRenderFeature> OperationReport for TypedReport<F> {
    fn as_any(&self) -> &dyn Any {
        &self.value
    }
}

struct TypedCapture<F: WgpuRenderFeature> {
    capture: Arc<F::Capture>,
    identity: Arc<FeatureInstanceIdentity>,
    definition: Arc<FeatureDefinition<F>>,
}

impl<F: WgpuRenderFeature> CapturedOperation for TypedCapture<F> {
    fn identity(&self) -> &Arc<FeatureInstanceIdentity> {
        &self.identity
    }

    #[cfg(feature = "stock-ui")]
    fn capture_value(&self) -> &dyn Any {
        self.capture.as_ref()
    }

    fn requirements(&self) -> &WgpuDeviceRequirements {
        &self.definition.requirements
    }

    fn preflight(
        &self,
        context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        self.definition.feature.preflight(&self.capture, context)
    }

    fn prepare(
        &self,
        states: &mut NativeFeatureStates,
        epoch: u64,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Arc<dyn PreparedOperation>, WgpuRenderProgramError> {
        let id = self.identity.id;
        states
            .0
            .retain(|entry| entry.identity.id != id || entry.epoch == epoch);
        let index = if let Some(index) = states.0.iter().position(|entry| entry.identity.id == id) {
            index
        } else {
            if states.0.len() >= MAX_ACTIVE_NATIVE_FEATURE_STATES {
                return Err(WgpuRenderProgramError::ResourceCapacity);
            }
            if states.0.capacity() == 0 {
                states
                    .0
                    .try_reserve_exact(MAX_ACTIVE_NATIVE_FEATURE_STATES)
                    .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
            }
            let value = self.definition.feature.create_state(context)?;
            let index = states.0.len();
            states.0.push(NativeStateEntry {
                identity: Arc::clone(&self.identity),
                epoch,
                value: Arc::new(TypedNativeState::<F> {
                    value: Mutex::new(value),
                    _definition: Arc::clone(&self.definition),
                }),
            });
            index
        };
        let state = Arc::clone(&states.0[index].value);
        // Preparation may enqueue writes before returning an error. Retain its original cache
        // state before invoking native code; failed/panicking state is discarded from new use.
        context.retain(Arc::clone(&state))?;
        let typed = state
            .as_any()
            .downcast_ref::<TypedNativeState<F>>()
            .ok_or(WgpuRenderProgramError::FeatureState)?;
        let mut guard = typed
            .value
            .lock()
            .map_err(|_| WgpuRenderProgramError::FeatureState)?;
        let prepared = self
            .definition
            .feature
            .prepare(&mut guard, &self.capture, context)?;
        Ok(Arc::new(TypedPrepared::<F> {
            definition: Arc::clone(&self.definition),
            _capture: Arc::clone(&self.capture),
            prepared,
        }))
    }
}

struct TypedPrepared<F: WgpuRenderFeature> {
    prepared: F::Prepared,
    _capture: Arc<F::Capture>,
    definition: Arc<FeatureDefinition<F>>,
}

impl<F: WgpuRenderFeature> PreparedOperation for TypedPrepared<F> {
    fn encode(
        &self,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Arc<dyn OperationReport>, WgpuRenderProgramError> {
        self.definition
            .feature
            .encode(&self.prepared, context)
            .map(|report| {
                Arc::new(TypedReport::<F> {
                    value: report,
                    _definition: Arc::clone(&self.definition),
                }) as Arc<dyn OperationReport>
            })
    }
}
