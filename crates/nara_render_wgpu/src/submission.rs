//! Original-queue submission custody, independent of renderer namespaces and telemetry.
//!
//! The Host serializes uploads, native submission, and callback registration. A completion here
//! settles retained submission resources; it never establishes validation, output publication,
//! presentation, UI acceptance, or the continued validity of a device epoch.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicU8, Ordering};
use thiserror::Error;

// Browser WGPU handles belong to their JavaScript agent. Native resources may be retained by a
// callback on another polling thread; the local browser callback has no Send/Sync requirement.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait SubmissionResources: Send + Sync + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + 'static> SubmissionResources for T {}
#[cfg(target_arch = "wasm32")]
pub(crate) trait SubmissionResources: 'static {}
#[cfg(target_arch = "wasm32")]
impl<T: 'static> SubmissionResources for T {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrefixState {
    Pending,
    Releasing,
    Completed,
    Abandoned,
}

struct RetainedState {
    state: PrefixState,
    keep_alive: Option<RetainedResourceSet>,
    callback_abandoned: bool,
    uncertain_index: bool,
}

pub(crate) const MAX_SUBMISSION_RESOURCES: usize = 65_536;

#[derive(Debug)]
pub(crate) enum SubmissionCapacity {
    Limit,
    Allocation,
}

/// Flat original custody. Releasing a large valid graph must not recurse once per resource.
struct RetainedResourceSet(Vec<Box<dyn SubmissionResources>>);

impl RetainedResourceSet {
    fn new<T: SubmissionResources>(value: T) -> Self {
        Self(vec![Box::new(value)])
    }

    fn retain<T: SubmissionResources>(&mut self, value: T) -> Result<(), (SubmissionCapacity, T)> {
        if self.0.len() >= MAX_SUBMISSION_RESOURCES {
            return Err((SubmissionCapacity::Limit, value));
        }
        if self.0.try_reserve_exact(1).is_err() {
            return Err((SubmissionCapacity::Allocation, value));
        }
        self.0.push(Box::new(value));
        Ok(())
    }
}

impl Drop for RetainedResourceSet {
    fn drop(&mut self) {
        // Prepared/native work retires before earlier captures and definition dependencies.
        while let Some(value) = self.0.pop() {
            drop(value);
        }
    }
}

struct RetainedResources {
    inner: Mutex<RetainedState>,
    // WGPU requires a Send callback even for browser handles. Only this CPU notification crosses
    // that callback seam; the agent-local owner releases its actual resources when observed.
    #[cfg(target_arch = "wasm32")]
    notification: Arc<AtomicU8>,
}

impl RetainedResources {
    fn state(&self) -> MutexGuard<'_, RetainedState> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn is_complete(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        self.observe_notification();
        self.state().state == PrefixState::Completed
    }

    fn callback_abandoned(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        self.observe_notification();
        self.state().callback_abandoned
    }

    #[cfg(target_arch = "wasm32")]
    fn observe_notification(&self) {
        match self.notification.load(Ordering::Acquire) {
            1 => {
                self.settle();
            }
            2 => self.abandon(),
            _ => {}
        }
    }

    fn abandon(&self) {
        let mut state = self.state();
        if state.state != PrefixState::Completed {
            state.callback_abandoned = true;
            state.state = PrefixState::Abandoned;
        }
    }

    fn settle(&self) -> bool {
        let keep_alive = {
            let mut state = self.state();
            match state.state {
                PrefixState::Completed => return true,
                PrefixState::Releasing => return false,
                PrefixState::Pending | PrefixState::Abandoned => {}
            }
            state.state = PrefixState::Releasing;
            state.keep_alive.take()
        };
        // Do not hold our lock across arbitrary resource destructors. Other token clones must
        // continue to observe incomplete until all original resources have actually released.
        drop(keep_alive);
        self.state().state = PrefixState::Completed;
        true
    }
}

impl Drop for RetainedResources {
    fn drop(&mut self) {
        #[cfg(target_arch = "wasm32")]
        self.observe_notification();
        let state = self
            .inner
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        if state.state != PrefixState::Completed {
            if let Some(keep_alive) = state.keep_alive.take() {
                // No callback or polling token remains to establish physical completion.
                std::mem::forget(keep_alive);
            }
        }
    }
}

struct OriginalSubmissionOwner {
    native: Option<(wgpu::Device, wgpu::Queue)>,
    retained: Arc<RetainedResources>,
}

impl OriginalSubmissionOwner {
    fn device(&self) -> &wgpu::Device {
        &self.native.as_ref().expect("original pair is retained").0
    }

    fn queue(&self) -> &wgpu::Queue {
        &self.native.as_ref().expect("original pair is retained").1
    }
}

impl Drop for OriginalSubmissionOwner {
    fn drop(&mut self) {
        if !self.retained.is_complete() {
            if let Some(native) = self.native.take() {
                // An unfinished reservation, or an abandoned callback with no remaining token,
                // has no reaper. Keep the original native pair instead of destroying uncertainty.
                std::mem::forget(native);
            }
        }
    }
}

struct CompletionCallback {
    #[cfg(not(target_arch = "wasm32"))]
    retained: Arc<RetainedResources>,
    // Native handles are Send + Sync. Retain the original pair even if all public token clones
    // are dropped while this callback is pending. Browser handles cannot enter a Send closure;
    // their token retains the original pair and resources, while callbacks carry only notification.
    #[cfg(not(target_arch = "wasm32"))]
    _original: Arc<OriginalSubmissionOwner>,
    #[cfg(target_arch = "wasm32")]
    notification: Arc<AtomicU8>,
}

impl CompletionCallback {
    fn complete(self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.retained.settle();
        #[cfg(target_arch = "wasm32")]
        self.notification.store(1, Ordering::Release);
    }
}

impl Drop for CompletionCallback {
    fn drop(&mut self) {
        // In locked wgpu-core, a mapping callback can unwind out of UserClosures::fire before
        // submission callbacks run. Their destruction is abandonment, not completion.
        #[cfg(not(target_arch = "wasm32"))]
        self.retained.abandon();
        #[cfg(target_arch = "wasm32")]
        {
            let _ = self
                .notification
                .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}

/// Retains original resources before entering a native submission attempt.
///
/// The Device and Queue must be the inseparable pair from the same device request. Construct this
/// before preparation can enqueue native uploads. Add prepared draw resources with `retain`, then
/// call `submitted` immediately after that Queue's `submit`, or `bind_current_prefix` after a
/// native SDK route that owns its submissions, inside the same Host-serialized segment. No
/// intervening producer may submit, enqueue writes, or leave a `QueueWriteBufferView` whose later
/// Drop enqueues writes inside that segment. Existing WGPU callbacks serviced there must not
/// reenter submission.
///
/// Finish every reservation with one of those bindings, including after failed preparation or SDK
/// execution that may have enqueued uploads. Dropping an unfinished reservation conservatively
/// retains its original native pair and resources: this object cannot determine whether the native
/// call accepted work before a panic or an early return. Without a bound polling token that fallback
/// has no recovery entry or background reaper; it is not cleanup or completion.
#[must_use = "bind the original queue prefix after submission or native SDK execution"]
pub(crate) struct WgpuSubmissionReservation {
    original: Arc<OriginalSubmissionOwner>,
}

impl WgpuSubmissionReservation {
    pub(crate) fn modeled_metadata_bytes<T: SubmissionResources>() -> usize {
        std::mem::size_of::<OriginalSubmissionOwner>()
            + std::mem::size_of::<RetainedResources>()
            + std::mem::size_of::<T>()
            + std::mem::size_of::<Box<dyn SubmissionResources>>()
            + 4 * std::mem::size_of::<usize>()
    }

    pub(crate) fn modeled_retain_bytes<T: SubmissionResources>() -> usize {
        std::mem::size_of::<T>() + std::mem::size_of::<Box<dyn SubmissionResources>>()
    }

    pub(crate) fn new<T: SubmissionResources>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        keep_alive: T,
    ) -> Self {
        Self {
            original: Arc::new(OriginalSubmissionOwner {
                native: Some((device.clone(), queue.clone())),
                retained: Arc::new(RetainedResources {
                    inner: Mutex::new(RetainedState {
                        state: PrefixState::Pending,
                        keep_alive: Some(RetainedResourceSet::new(keep_alive)),
                        callback_abandoned: false,
                        uncertain_index: false,
                    }),
                    #[cfg(target_arch = "wasm32")]
                    notification: Arc::new(AtomicU8::new(0)),
                }),
            }),
        }
    }

    /// Adds prepared resources to the original pre-upload custody before native submission.
    ///
    /// A bounded flat container retains every earlier value without recursive destruction.
    /// Admit this entry before enqueuing its native use. Bind
    /// the original prefix even when preparation or SDK execution fails after enqueuing writes.
    pub(crate) fn retain<T: SubmissionResources>(
        &mut self,
        value: T,
    ) -> Result<(), (SubmissionCapacity, T)> {
        let mut state = self.original.retained.state();
        state
            .keep_alive
            .as_mut()
            .expect("reservation is retained")
            .retain(value)
    }

    /// Binds the original Queue's just-returned submission attempt, exactly once.
    ///
    /// The caller must preserve the serialized segment described on this type. WGPU 30 registers
    /// this callback against the current submitted prefix and may flush pending writes during
    /// registration. Finish this attempt's writes before `submit`; do not enqueue a later
    /// producer's writes between `submit` and this call. Later submissions do not move the bound
    /// callback. Polling the returned token never flushes or submits anything.
    ///
    /// WGPU can return a submission index on a validation failure. Neither that index nor the
    /// completion callback proves success. Native scopes and epoch validation remain the caller's
    /// responsibility, including on failure; do not abort custody for a possibly accepted attempt.
    /// This callback covers all earlier accepted work on the same Queue, so this token may replace
    /// that producer's latest token even when the new attempt failed validation. Earlier resource
    /// callbacks remain independently owned. Retain outstanding predecessor tokens separately until
    /// settlement so a subsequently abandoned callback can still recover; this token does not take
    /// over another callback's CPU custody.
    pub(crate) fn submitted(self, index: wgpu::SubmissionIndex) -> WgpuSubmissionPrefix {
        self.bind(Some(index))
    }

    /// Binds the current original Queue prefix after a native SDK route without an exposed index.
    ///
    /// Reserve before entering the route and call this on success or failure before leaving the
    /// same Host-serialized segment. In locked wgpu-core, on_submitted_work_done flushes pending
    /// writes before binding the callback to the current last active submission. Consequently this
    /// also covers uploads left by a failed SDK route without a caller-issued empty Queue::submit.
    /// No unrelated writes or submissions may intervene, and deferred write views must already
    /// have been dropped. Later producer work does not extend the registered callback's prefix.
    ///
    /// No native index is inferred from an SDK report. Ordinary settlement still uses the actual
    /// callback; if abandoned, explicit recovery requires QueueEmpty on the original device. This
    /// resource settlement says nothing about SDK validation, actual presentation, output, or UI
    /// success. Retain the returned token with all other outstanding prefixes until it settles.
    pub(crate) fn bind_current_prefix(self) -> WgpuSubmissionPrefix {
        self.bind(None)
    }

    fn bind(self, index: Option<wgpu::SubmissionIndex>) -> WgpuSubmissionPrefix {
        self.original.retained.state().uncertain_index = index.is_none();
        let callback = CompletionCallback {
            #[cfg(not(target_arch = "wasm32"))]
            retained: Arc::clone(&self.original.retained),
            #[cfg(not(target_arch = "wasm32"))]
            _original: Arc::clone(&self.original),
            #[cfg(target_arch = "wasm32")]
            notification: Arc::clone(&self.original.retained.notification),
        };
        self.original
            .queue()
            .on_submitted_work_done(move || callback.complete());

        // Until registration returns, the reservation still owns the original pair and resources.
        // Unwinding cannot mark that custody complete or free possibly submitted resources.
        WgpuSubmissionPrefix {
            original: self.original,
            index,
        }
    }
}

/// Failure to observe or explicitly recover an original submission prefix.
#[doc(hidden)]
#[derive(Debug, Error)]
pub enum WgpuSubmissionPrefixError {
    #[error(
        "the original submission callback was abandoned; recover the retained prefix explicitly"
    )]
    CallbackAbandoned,
    #[error(
        "abandoned submission recovery requires a native backend with submission-index polling"
    )]
    NativeRecoveryUnavailable,
    #[error(transparent)]
    Poll(#[from] wgpu::PollError),
}

/// A frozen submission prefix on the original Device and Queue.
///
/// Clones observe the same monotonic settlement; they never follow a producer's newer submission.
/// Keep this token outside namespace cleanup until it settles. A later token bound on the same
/// serialized original Queue covers earlier accepted work even if the later native attempt failed.
/// Retain every outstanding token until it settles, including predecessors of the producer's latest
/// token: their callbacks can be abandoned after replacement and each needs a recovery owner. A
/// newer token does not join arbitrary CPU work or take over earlier callbacks' resource custody.
/// An output or composition sampled by another submission needs its own retention; retiring its
/// producer does not end that other use.
///
/// Dropping a token does not mark it complete or cancel its callback. The callback retains the
/// resources independently of token clones. On native, the pending callback also retains the
/// original pair. If it is discarded, remaining tokens retain recovery custody and report explicit
/// abandonment. Without any callback or token, uncertain resources and the original pair are
/// conservatively retained with no reaper. Browser callbacks carry only a CPU notification and
/// require event-loop progress; the local token retains GPU resources and observes that signal.
/// Dropping its last token before completion retains the unproven original pair and resources.
///
/// Device loss may invalidate every associated output before this prefix settles. Conversely a
/// settled callback, including WGPU's error/loss cleanup, must never restore output or UI authority.
#[doc(hidden)]
#[must_use = "retain and poll the original prefix independently of producer namespace lifetime"]
#[derive(Clone)]
pub struct WgpuSubmissionPrefix {
    original: Arc<OriginalSubmissionOwner>,
    index: Option<wgpu::SubmissionIndex>,
}

impl fmt::Debug for WgpuSubmissionPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.original.retained.state();
        formatter
            .debug_struct("WgpuSubmissionPrefix")
            .field("index", &self.index)
            .field("state", &state.state)
            .field("callback_abandoned", &state.callback_abandoned)
            .field("uncertain_index", &state.uncertain_index)
            .finish_non_exhaustive()
    }
}

impl WgpuSubmissionPrefix {
    /// Services the original device once, without waiting, flushing writes, or submitting work.
    ///
    /// Only the actual callback settles the ordinary path; QueueEmpty is deliberately irrelevant
    /// while other producers continue submitting. A discarded callback reports abandonment instead
    /// of remaining pending forever. Poll errors preserve custody. WGPU can report other native
    /// failures through its fatal handler; this type does not promise general unwind recovery.
    pub fn poll(&self) -> Result<bool, WgpuSubmissionPrefixError> {
        if self.is_complete() {
            return Ok(true);
        }
        if self.callback_abandoned() {
            return Err(WgpuSubmissionPrefixError::CallbackAbandoned);
        }
        self.original.device().poll(wgpu::PollType::Poll)?;
        if !self.is_complete() && self.callback_abandoned() {
            return Err(WgpuSubmissionPrefixError::CallbackAbandoned);
        }
        Ok(self.is_complete())
    }

    /// Reports resource settlement only, without servicing the device or asserting output success.
    pub fn is_complete(&self) -> bool {
        self.original.retained.is_complete()
    }

    /// Reports callback abandonment, including after explicit recovery successfully settles it.
    pub fn callback_abandoned(&self) -> bool {
        self.original.retained.callback_abandoned()
    }

    /// Explicitly attempts nonblocking native recovery after callback abandonment.
    ///
    /// Waits only for this frozen native index with a zero timeout. WaitSucceeded or QueueEmpty
    /// establishes resource settlement; a timeout keeps custody and returns false. If the rejected
    /// attempt's index cannot be waited on, or an SDK binding has no index, use the explicit
    /// uncertainty branch: only QueueEmpty on the original device can then settle it. That
    /// exceptional branch may depend on unrelated producers draining. It never submits, flushes,
    /// chases a newer index, or interprets device loss as completion. Native fatal poll failures
    /// remain failures, not evidence of settlement.
    ///
    /// Without observed abandonment this uses the ordinary callback path. Browser polling cannot
    /// establish this native fence evidence and reports NativeRecoveryUnavailable instead.
    pub fn recover_abandoned(&self) -> Result<bool, WgpuSubmissionPrefixError> {
        if self.is_complete() {
            return Ok(true);
        }
        if !self.callback_abandoned() {
            return self.poll();
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err(WgpuSubmissionPrefixError::NativeRecoveryUnavailable)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let uncertain_index = self.original.retained.state().uncertain_index;
            if let (false, Some(index)) = (uncertain_index, self.index.as_ref()) {
                match self.original.device().poll(wgpu::PollType::Wait {
                    submission_index: Some(index.clone()),
                    timeout: Some(std::time::Duration::ZERO),
                }) {
                    Ok(wgpu::PollStatus::WaitSucceeded | wgpu::PollStatus::QueueEmpty) => {
                        return Ok(self.original.retained.settle());
                    }
                    Ok(wgpu::PollStatus::Poll) | Err(wgpu::PollError::Timeout) => return Ok(false),
                    Err(wgpu::PollError::WrongSubmissionIndex(..)) => {
                        self.original.retained.state().uncertain_index = true;
                    }
                }
            }
            match self.original.device().poll(wgpu::PollType::Poll)? {
                wgpu::PollStatus::QueueEmpty => Ok(self.original.retained.settle()),
                wgpu::PollStatus::Poll | wgpu::PollStatus::WaitSucceeded => Ok(false),
            }
        }
    }

    /// Returns the opaque index from the bound attempt, or None for a native SDK prefix binding.
    ///
    /// The caller's native validation determines whether that attempt was accepted. The index is
    /// not a process-wide identity and is not portable across devices. Ordinary `poll` does not
    /// wait on it; explicit abandoned recovery handles a rejected index conservatively.
    pub fn submission_index(&self) -> Option<&wgpu::SubmissionIndex> {
        self.index.as_ref()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    use super::*;

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn flat_original_custody_releases_its_capacity_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let drops = Arc::new(AtomicUsize::new(0));
                let mut values = RetainedResourceSet::new(DropProbe(Arc::clone(&drops)));
                for _ in 1..MAX_SUBMISSION_RESOURCES {
                    assert!(values.retain(DropProbe(Arc::clone(&drops))).is_ok());
                }
                assert_eq!(drops.load(Ordering::Acquire), 0);
                let (_, rejected) = values.retain(DropProbe(Arc::clone(&drops))).err().unwrap();
                assert_eq!(values.0.len(), MAX_SUBMISSION_RESOURCES);
                drop(rejected);
                assert_eq!(drops.load(Ordering::Acquire), 1);
                drop(values);
                assert_eq!(drops.load(Ordering::Acquire), MAX_SUBMISSION_RESOURCES + 1);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn original_resources_retire_in_reverse_dependency_order() {
        struct OrderedDrop(u8, Arc<Mutex<Vec<u8>>>);
        impl Drop for OrderedDrop {
            fn drop(&mut self) {
                self.1.lock().unwrap().push(self.0);
            }
        }
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut values = RetainedResourceSet::new(OrderedDrop(1, Arc::clone(&order)));
        assert!(values.retain(OrderedDrop(2, Arc::clone(&order))).is_ok());
        assert!(values.retain(OrderedDrop(3, Arc::clone(&order))).is_ok());
        drop(values);
        assert_eq!(*order.lock().unwrap(), [3, 2, 1]);
    }

    struct NativeGpu {
        device: wgpu::Device,
        queue: wgpu::Queue,
    }

    impl NativeGpu {
        fn new() -> Self {
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let force_fallback_adapter = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref()
            {
                Ok("1") => true,
                Ok("0") | Err(std::env::VarError::NotPresent) => false,
                value => panic!("invalid NARA_WGPU_FORCE_FALLBACK: {value:?}"),
            };
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter,
                    ..Default::default()
                }))
                .expect("submission custody requires the selected native GPU");
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                    .expect("submission custody requires a native device");
            Self { device, queue }
        }

        fn buffer(&self) -> wgpu::Buffer {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("submission-custody-copy"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        }

        fn submit(&self, buffer: &wgpu::Buffer, drops: &Arc<AtomicUsize>) -> WgpuSubmissionPrefix {
            let reservation = WgpuSubmissionReservation::new(
                &self.device,
                &self.queue,
                (buffer.clone(), DropProbe(Arc::clone(drops))),
            );
            self.queue.write_buffer(buffer, 0, &[7; 16]);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.clear_buffer(buffer, 8, None);
            let index = self.queue.submit([encoder.finish()]);
            reservation.submitted(index)
        }
    }

    fn poll_until_complete(prefix: &WgpuSubmissionPrefix) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !prefix
            .poll()
            .expect("the original prefix must remain pollable")
        {
            assert!(
                Instant::now() < deadline,
                "submission prefix did not settle"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn discarded_submission_callback_recovers_on_its_original_native_pair() {
        for (rejected, sdk_binding) in [(false, false), (false, true)] {
            let gpu = NativeGpu::new();
            let buffer = gpu.buffer();
            let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("submission-custody-panicking-map"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let drops = Arc::new(AtomicUsize::new(0));
            let reservation = WgpuSubmissionReservation::new(
                &gpu.device,
                &gpu.queue,
                (
                    buffer.clone(),
                    readback.clone(),
                    DropProbe(Arc::clone(&drops)),
                ),
            );
            let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.clear_buffer(&buffer, 0, None);
            if !rejected {
                encoder.copy_buffer_to_buffer(&buffer, 0, &readback, 0, 16);
            }
            let commands = encoder.finish();
            if rejected {
                buffer.destroy();
            }
            let index = gpu.queue.submit([commands]);
            assert_eq!(pollster::block_on(validation.pop()).is_some(), rejected);

            // Register the mapping before our submission callback. Locked UserClosures::fire
            // calls mappings first, then drops the pending submission callbacks on this unwind.
            let mapping_calls = Arc::new(AtomicUsize::new(0));
            let mapping_called = Arc::clone(&mapping_calls);
            readback.map_async(wgpu::MapMode::Read, .., move |result| {
                result.expect("the native mapping must succeed before the injected unwind");
                mapping_called.fetch_add(1, Ordering::AcqRel);
                panic!("injected mapping-callback unwind");
            });
            let prefix = if sdk_binding {
                reservation.bind_current_prefix()
            } else {
                reservation.submitted(index)
            };
            assert_eq!(prefix.submission_index().is_none(), sdk_binding);
            let clone = prefix.clone();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    prefix.original.device().poll(wgpu::PollType::Poll)
                }));
                match result {
                    Err(_) => break,
                    Ok(result) => result.expect("native maintenance failed before the unwind"),
                };
                assert!(
                    Instant::now() < deadline,
                    "native mapping callback never ran"
                );
                std::thread::yield_now();
            }
            assert_eq!(mapping_calls.load(Ordering::Acquire), 1);
            assert!(prefix.callback_abandoned());
            assert!(clone.callback_abandoned());
            assert!(!prefix.is_complete());
            assert_eq!(drops.load(Ordering::Acquire), 0);
            assert!(matches!(
                prefix.poll(),
                Err(WgpuSubmissionPrefixError::CallbackAbandoned)
            ));
            readback.unmap();
            drop(buffer);
            drop(readback);
            drop(gpu);

            // The ordinary namespace and callback are gone. Only the frozen token can recover
            // the original resources. The rejected-index case must take the explicit queue-empty
            // uncertainty branch, as must a binding without an exposed SDK index. An accepted
            // exposed index instead uses its own zero-time fence wait.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !prefix.recover_abandoned().unwrap() {
                assert!(
                    Instant::now() < deadline,
                    "abandoned prefix could not recover"
                );
                std::thread::yield_now();
            }
            assert_eq!(
                prefix.original.retained.state().uncertain_index,
                rejected || sdk_binding
            );
            assert_eq!(drops.load(Ordering::Acquire), 1);
            assert!(clone.is_complete());
            assert!(
                clone.callback_abandoned(),
                "preserve the failure observation"
            );
            assert!(clone.poll().unwrap());
            assert!(clone.recover_abandoned().unwrap());
        }
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn rejected_submission_index_recovers_after_controlled_callback_abandonment() {
        let gpu = NativeGpu::new();
        let buffer = gpu.buffer();
        let drops = Arc::new(AtomicUsize::new(0));
        let reservation = WgpuSubmissionReservation::new(
            &gpu.device,
            &gpu.queue,
            (buffer.clone(), DropProbe(Arc::clone(&drops))),
        );
        let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.clear_buffer(&buffer, 0, None);
        let commands = encoder.finish();
        buffer.destroy();
        let index = gpu.queue.submit([commands]);
        assert!(pollster::block_on(validation.pop()).is_some());
        // A rejected submission has no guaranteed active GPU batch with which to co-dispatch a
        // mapping panic. Model discarded notification explicitly here, while using the actual
        // rejected native index. The preceding test qualifies real callback destruction.
        let callback = CompletionCallback {
            retained: Arc::clone(&reservation.original.retained),
            _original: Arc::clone(&reservation.original),
        };
        drop(callback);
        let prefix = WgpuSubmissionPrefix {
            original: reservation.original,
            index: Some(index),
        };
        assert!(matches!(
            prefix.poll(),
            Err(WgpuSubmissionPrefixError::CallbackAbandoned)
        ));
        drop(buffer);
        drop(gpu);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !prefix.recover_abandoned().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(prefix.original.retained.state().uncertain_index);
        assert!(prefix.callback_abandoned());
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn original_prefix_outlives_namespace_and_reads_actual_submitted_bytes() {
        let gpu = NativeGpu::new();
        let buffer = gpu.buffer();
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("submission-custody-readback"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let drops = Arc::new(AtomicUsize::new(0));
        let prepared_drops = Arc::new(AtomicUsize::new(0));
        let mut reservation = WgpuSubmissionReservation::new(
            &gpu.device,
            &gpu.queue,
            (buffer.clone(), DropProbe(Arc::clone(&drops))),
        );
        gpu.queue.write_buffer(&buffer, 0, &[7; 16]);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.clear_buffer(&buffer, 8, None);
        encoder.copy_buffer_to_buffer(&buffer, 0, &readback, 0, 16);
        assert!(
            reservation
                .retain((readback.clone(), DropProbe(Arc::clone(&prepared_drops))))
                .is_ok()
        );
        let index = gpu.queue.submit([encoder.finish()]);
        let prefix = reservation.submitted(index);
        let clone = prefix.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
        drop(buffer);
        drop(gpu);
        drop(prefix);

        poll_until_complete(&clone);
        assert!(clone.is_complete());
        assert_eq!(drops.load(Ordering::Acquire), 1);
        assert_eq!(prepared_drops.load(Ordering::Acquire), 1);
        // The mapping was registered after the prefix callback; service that separate native
        // obligation explicitly rather than treating prefix settlement as mapping success.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            clone.original.device().poll(wgpu::PollType::Poll).unwrap();
            match receiver.try_recv() {
                Ok(result) => {
                    result.unwrap();
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "readback mapping did not finish");
                    std::thread::yield_now();
                }
                Err(error) => panic!("readback callback disappeared: {error}"),
            }
        }
        {
            let bytes = readback.get_mapped_range(..).unwrap();
            assert_eq!(&bytes[..8], &[7; 8]);
            assert_eq!(&bytes[8..], &[0; 8]);
        }
        readback.unmap();
        assert!(clone.poll().unwrap(), "settlement is monotonic");
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn dropping_prefix_handles_does_not_cancel_original_resource_retention() {
        let gpu = NativeGpu::new();
        let buffer = gpu.buffer();
        let drops = Arc::new(AtomicUsize::new(0));
        let prefix = gpu.submit(&buffer, &drops);
        let clone = prefix.clone();
        drop(prefix);
        drop(clone);
        drop(buffer);

        let deadline = Instant::now() + Duration::from_secs(5);
        while drops.load(Ordering::Acquire) == 0 {
            gpu.device.poll(wgpu::PollType::Poll).unwrap();
            assert!(
                Instant::now() < deadline,
                "original callback did not release custody"
            );
            std::thread::yield_now();
        }
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn producer_prefix_settles_while_another_producer_keeps_submitting() {
        let gpu = NativeGpu::new();
        let a = gpu.buffer();
        let b = gpu.buffer();
        let a_drops = Arc::new(AtomicUsize::new(0));
        let b_drops = Arc::new(AtomicUsize::new(0));
        let prefix = gpu.submit(&a, &a_drops);
        let clone = prefix.clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut other = gpu.submit(&b, &b_drops);
        while !prefix.poll().unwrap() {
            // Keep B active while checking A's frozen callback. B's callbacks retain their own
            // resources even when its diagnostic token is replaced. Do not wait for QueueEmpty.
            other = gpu.submit(&b, &b_drops);
            assert!(
                Instant::now() < deadline,
                "A waited for continuously renewed B work"
            );
        }
        assert!(clone.is_complete());
        assert_eq!(a_drops.load(Ordering::Acquire), 1);
        let previous_b_drops = b_drops.load(Ordering::Acquire);
        poll_until_complete(&other);
        let successor = gpu.submit(&b, &b_drops);
        poll_until_complete(&successor);
        assert!(b_drops.load(Ordering::Acquire) > previous_b_drops);
        assert!(prefix.poll().unwrap());
        assert!(clone.poll().unwrap());
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn original_prefix_remains_pollable_after_device_destruction() {
        let gpu = NativeGpu::new();
        let buffer = gpu.buffer();
        let drops = Arc::new(AtomicUsize::new(0));
        let prefix = gpu.submit(&buffer, &drops);
        gpu.device.destroy();
        drop(buffer);
        drop(gpu);
        poll_until_complete(&prefix);
        assert_eq!(drops.load(Ordering::Acquire), 1);
        // This asserts resource settlement only. Destroyed device output cannot become current.
        assert!(prefix.is_complete());
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn queued_preparation_uploads_settle_through_current_prefix_without_empty_submission() {
        let gpu = NativeGpu::new();
        let buffer = gpu.buffer();
        let drops = Arc::new(AtomicUsize::new(0));
        let reservation = WgpuSubmissionReservation::new(
            &gpu.device,
            &gpu.queue,
            (buffer.clone(), DropProbe(Arc::clone(&drops))),
        );
        gpu.queue.write_buffer(&buffer, 0, &[7; 16]);
        // A failed native route need not yield an encodable draw or a submission index. Locked
        // callback registration flushes these uploads once without a caller-issued empty submit.
        let prefix = reservation.bind_current_prefix();
        assert!(prefix.submission_index().is_none());
        drop(buffer);
        drop(gpu);
        poll_until_complete(&prefix);
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn rejected_successor_prefix_replaces_the_old_token_without_implying_success() {
        let gpu = NativeGpu::new();
        let previous_buffer = gpu.buffer();
        let previous_drops = Arc::new(AtomicUsize::new(0));
        let previous = gpu.submit(&previous_buffer, &previous_drops);
        drop(previous);
        drop(previous_buffer);
        let buffer = gpu.buffer();
        let drops = Arc::new(AtomicUsize::new(0));
        let reservation = WgpuSubmissionReservation::new(
            &gpu.device,
            &gpu.queue,
            (buffer.clone(), DropProbe(Arc::clone(&drops))),
        );
        let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.clear_buffer(&buffer, 0, None);
        let commands = encoder.finish();
        buffer.destroy();
        let index = gpu.queue.submit([commands]);
        let prefix = reservation.submitted(index);
        let rejected = pollster::block_on(validation.pop());
        assert!(
            rejected.is_some(),
            "destroyed native input must be rejected"
        );
        poll_until_complete(&prefix);
        assert_eq!(previous_drops.load(Ordering::Acquire), 1);
        assert_eq!(drops.load(Ordering::Acquire), 1);
        // No wait on the potentially rejected index, output receipt, or UI success is produced.
        assert!(prefix.is_complete());
    }
}
