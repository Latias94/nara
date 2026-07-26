use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

pub const MAX_PENDING_FRAME_COMPLETIONS: u32 = 256;
pub const MAX_BUFFERED_FRAME_COMPLETIONS: u32 = 4_096;
pub const MAX_ADAPTER_SUMMARY_FIELD_BYTES: usize = 256;

const DEFAULT_PENDING_FRAME_COMPLETIONS: u32 = 64;
const DEFAULT_BUFFERED_FRAME_COMPLETIONS: u32 = 256;

/// Bounded diagnostic identity for the adapter selected by this backend instance.
///
/// The summary deliberately contains no `wgpu` types. Dynamic driver-provided text has control
/// characters replaced and is truncated on a UTF-8 boundary so evidence writers can retain it
/// without introducing an unbounded backend string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuAdapterSummary {
    name: String,
    vendor: u32,
    device: u32,
    device_type: &'static str,
    backend: &'static str,
    driver: String,
    driver_info: String,
}

impl WgpuAdapterSummary {
    pub(crate) fn from_parts(
        name: &str,
        vendor: u32,
        device: u32,
        device_type: &'static str,
        backend: &'static str,
        driver: &str,
        driver_info: &str,
    ) -> Self {
        Self {
            name: bounded_adapter_text(name),
            vendor,
            device,
            device_type,
            backend,
            driver: bounded_adapter_text(driver),
            driver_info: bounded_adapter_text(driver_info),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn vendor(&self) -> u32 {
        self.vendor
    }

    #[must_use]
    pub const fn device(&self) -> u32 {
        self.device
    }

    #[must_use]
    pub const fn device_type(&self) -> &'static str {
        self.device_type
    }

    #[must_use]
    pub const fn backend(&self) -> &'static str {
        self.backend
    }

    #[must_use]
    pub fn driver(&self) -> &str {
        &self.driver
    }

    #[must_use]
    pub fn driver_info(&self) -> &str {
        &self.driver_info
    }
}

fn bounded_adapter_text(value: &str) -> String {
    let mut bounded = String::with_capacity(value.len().min(MAX_ADAPTER_SUMMARY_FIELD_BYTES));
    for character in value.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if bounded.len().saturating_add(character.len_utf8()) > MAX_ADAPTER_SUMMARY_FIELD_BYTES {
            break;
        }
        bounded.push(character);
    }
    bounded
}

/// Finite opt-in limits for backend-owned GPU completion observations.
///
/// This controls diagnostic evidence only. It does not change queue submission, presentation, or
/// frame scheduling. While enabled, the backend never registers more than
/// `max_pending_frame_completions` completion callbacks and never retains more than
/// `max_buffered_frame_completions` completed samples. The same bounded callbacks retire tracked
/// per-frame resource payloads, but sampling and resource retirement keep independent generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuBackendTelemetryConfig {
    max_pending_frame_completions: u32,
    max_buffered_frame_completions: u32,
}

impl WgpuBackendTelemetryConfig {
    pub fn new(
        max_pending_frame_completions: u32,
        max_buffered_frame_completions: u32,
    ) -> Result<Self, WgpuTelemetryConfigError> {
        if max_pending_frame_completions == 0 {
            return Err(WgpuTelemetryConfigError::ZeroPendingCapacity);
        }
        if max_buffered_frame_completions == 0 {
            return Err(WgpuTelemetryConfigError::ZeroCompletedCapacity);
        }
        if max_pending_frame_completions > MAX_PENDING_FRAME_COMPLETIONS {
            return Err(WgpuTelemetryConfigError::CapacityExceedsLimit {
                category: "pending-frame-completions",
                requested: max_pending_frame_completions,
                maximum: MAX_PENDING_FRAME_COMPLETIONS,
            });
        }
        if max_buffered_frame_completions > MAX_BUFFERED_FRAME_COMPLETIONS {
            return Err(WgpuTelemetryConfigError::CapacityExceedsLimit {
                category: "buffered-frame-completions",
                requested: max_buffered_frame_completions,
                maximum: MAX_BUFFERED_FRAME_COMPLETIONS,
            });
        }
        Ok(Self {
            max_pending_frame_completions,
            max_buffered_frame_completions,
        })
    }

    #[must_use]
    pub const fn max_pending_frame_completions(self) -> u32 {
        self.max_pending_frame_completions
    }

    #[must_use]
    pub const fn max_buffered_frame_completions(self) -> u32 {
        self.max_buffered_frame_completions
    }
}

impl Default for WgpuBackendTelemetryConfig {
    fn default() -> Self {
        Self {
            max_pending_frame_completions: DEFAULT_PENDING_FRAME_COMPLETIONS,
            max_buffered_frame_completions: DEFAULT_BUFFERED_FRAME_COMPLETIONS,
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum WgpuTelemetryConfigError {
    #[error("pending frame-completion capacity must be non-zero")]
    ZeroPendingCapacity,
    #[error("completed frame-sample capacity must be non-zero")]
    ZeroCompletedCapacity,
    #[error("{category} capacity {requested} exceeds the maximum {maximum}")]
    CapacityExceedsLimit {
        category: &'static str,
        requested: u32,
        maximum: u32,
    },
}

/// One App-frame-start-to-GPU-completion duration for an exact submitted frame and device epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuFrameCompletionSample {
    frame_index: u64,
    device_epoch: u64,
    duration_ns: u64,
}

impl WgpuFrameCompletionSample {
    const fn new(frame_index: u64, device_epoch: u64, duration_ns: u64) -> Self {
        Self {
            frame_index,
            device_epoch,
            duration_ns,
        }
    }

    #[must_use]
    pub const fn frame_index(self) -> u64 {
        self.frame_index
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn duration_ns(self) -> u64 {
        self.duration_ns
    }
}

/// Bounded completion-channel state for the current backend instance.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuFrameCompletionStats {
    enabled: bool,
    device_epoch: u64,
    pending_samples: u32,
    buffered_samples: u32,
    completed_samples: u64,
    lost_samples: u64,
}

impl WgpuFrameCompletionStats {
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn pending_samples(self) -> u32 {
        self.pending_samples
    }

    #[must_use]
    pub const fn buffered_samples(self) -> u32 {
        self.buffered_samples
    }

    #[must_use]
    pub const fn completed_samples(self) -> u64 {
        self.completed_samples
    }

    #[must_use]
    pub const fn lost_samples(self) -> u64 {
        self.lost_samples
    }
}

/// Backend-tracked logical GPU resource bytes for one device epoch.
///
/// Texture bytes include image-cache and fallback texture payloads. With telemetry enabled before
/// the first submission, instance-buffer bytes include all completion-correlated submitted quad
/// payloads still in flight plus any payload currently being prepared. Surface images, driver
/// allocation padding, pipelines, bind groups, samplers, and other unobservable driver residency
/// are excluded. Evidence requires both `lost_retirements == 0` and `pending_retirements == 0` at
/// its end boundary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuGpuResourceStats {
    device_epoch: u64,
    tracking_enabled: bool,
    pending_retirements: u32,
    lost_retirements: u64,
    current_texture_bytes: u64,
    current_instance_buffer_bytes: u64,
    current_total_bytes: u64,
    peak_texture_bytes: u64,
    peak_instance_buffer_bytes: u64,
    peak_total_bytes: u64,
}

impl WgpuGpuResourceStats {
    const fn for_device_epoch(device_epoch: u64, tracking_enabled: bool) -> Self {
        Self {
            device_epoch,
            tracking_enabled,
            pending_retirements: 0,
            lost_retirements: 0,
            current_texture_bytes: 0,
            current_instance_buffer_bytes: 0,
            current_total_bytes: 0,
            peak_texture_bytes: 0,
            peak_instance_buffer_bytes: 0,
            peak_total_bytes: 0,
        }
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    /// Whether completion-correlated instance-buffer tracking is enabled.
    #[must_use]
    pub const fn tracking_enabled(self) -> bool {
        self.tracking_enabled
    }

    /// Submitted resource payloads whose GPU-completion callback has not run yet.
    #[must_use]
    pub const fn pending_retirements(self) -> u32 {
        self.pending_retirements
    }

    /// Resource retirements that could not be observed exactly in this device epoch.
    ///
    /// Any non-zero value invalidates the completion-correlated resource peak as measurement
    /// evidence. Bytes from an untracked retirement remain conservatively charged until the
    /// device epoch ends so this condition cannot silently under-report current residency.
    #[must_use]
    pub const fn lost_retirements(self) -> u64 {
        self.lost_retirements
    }

    #[must_use]
    pub const fn current_texture_bytes(self) -> u64 {
        self.current_texture_bytes
    }

    #[must_use]
    pub const fn current_instance_buffer_bytes(self) -> u64 {
        self.current_instance_buffer_bytes
    }

    #[must_use]
    pub const fn current_total_bytes(self) -> u64 {
        self.current_total_bytes
    }

    #[must_use]
    pub const fn peak_texture_bytes(self) -> u64 {
        self.peak_texture_bytes
    }

    #[must_use]
    pub const fn peak_instance_buffer_bytes(self) -> u64 {
        self.peak_instance_buffer_bytes
    }

    #[must_use]
    pub const fn peak_total_bytes(self) -> u64 {
        self.peak_total_bytes
    }

    fn observe(&mut self, texture_bytes: u64, instance_buffer_bytes: u64) {
        let total_bytes = texture_bytes.saturating_add(instance_buffer_bytes);
        self.current_texture_bytes = texture_bytes;
        self.current_instance_buffer_bytes = instance_buffer_bytes;
        self.current_total_bytes = total_bytes;
        self.peak_texture_bytes = self.peak_texture_bytes.max(texture_bytes);
        self.peak_instance_buffer_bytes =
            self.peak_instance_buffer_bytes.max(instance_buffer_bytes);
        self.peak_total_bytes = self.peak_total_bytes.max(total_bytes);
    }

    fn clear_current(&mut self) {
        self.current_texture_bytes = 0;
        self.current_instance_buffer_bytes = 0;
        self.current_total_bytes = 0;
    }
}

#[derive(Debug)]
struct WgpuTelemetryState {
    config: WgpuBackendTelemetryConfig,
    completion_generation: u64,
    resource_generation: u64,
    completion: WgpuFrameCompletionStats,
    completed_frames: VecDeque<WgpuFrameCompletionSample>,
    prepared_instance_buffer_bytes: u64,
    in_flight_instance_buffer_bytes: u64,
    untracked_instance_buffer_bytes: u64,
    gpu_resources: WgpuGpuResourceStats,
}

impl Default for WgpuTelemetryState {
    fn default() -> Self {
        Self {
            config: WgpuBackendTelemetryConfig::default(),
            completion_generation: 0,
            resource_generation: 0,
            completion: WgpuFrameCompletionStats::default(),
            completed_frames: VecDeque::new(),
            prepared_instance_buffer_bytes: 0,
            in_flight_instance_buffer_bytes: 0,
            untracked_instance_buffer_bytes: 0,
            gpu_resources: WgpuGpuResourceStats::default(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct WgpuBackendTelemetry {
    shared: Arc<Mutex<WgpuTelemetryState>>,
}

impl WgpuBackendTelemetry {
    pub(crate) fn configure(&self, config: WgpuBackendTelemetryConfig) {
        let mut state = self.lock();
        let discarded = u64::from(state.completion.pending_samples)
            .saturating_add(u64::try_from(state.completed_frames.len()).unwrap_or(u64::MAX));
        state.completion.lost_samples = state.completion.lost_samples.saturating_add(discarded);
        state.completion.pending_samples = 0;
        state.completion.buffered_samples = 0;
        state.completed_frames.clear();
        state.config = config;
        state.completion_generation = next_generation(state.completion_generation);
        state.completion.enabled = true;
        state.gpu_resources.tracking_enabled = true;
        refresh_gpu_resources(&mut state);
    }

    pub(crate) fn begin_device_epoch(&self, device_epoch: u64) {
        let mut state = self.lock();
        if state.completion.device_epoch != device_epoch {
            let discarded = u64::from(state.completion.pending_samples)
                .saturating_add(u64::try_from(state.completed_frames.len()).unwrap_or(u64::MAX));
            state.completion.lost_samples = state.completion.lost_samples.saturating_add(discarded);
            state.completion.pending_samples = 0;
            state.completion.buffered_samples = 0;
            state.completed_frames.clear();
            state.completion_generation = next_generation(state.completion_generation);
            state.resource_generation = next_generation(state.resource_generation);
            state.completion.device_epoch = device_epoch;
            state.prepared_instance_buffer_bytes = 0;
            state.in_flight_instance_buffer_bytes = 0;
            state.untracked_instance_buffer_bytes = 0;
            state.gpu_resources =
                WgpuGpuResourceStats::for_device_epoch(device_epoch, state.completion.enabled);
        }
    }

    pub(crate) fn retire_device_epoch(&self) {
        let mut state = self.lock();
        state.completion.lost_samples = state
            .completion
            .lost_samples
            .saturating_add(u64::from(state.completion.pending_samples));
        state.completion.pending_samples = 0;
        state.completion_generation = next_generation(state.completion_generation);
        state.gpu_resources.lost_retirements = state
            .gpu_resources
            .lost_retirements
            .saturating_add(u64::from(state.gpu_resources.pending_retirements));
        state.gpu_resources.pending_retirements = 0;
        state.resource_generation = next_generation(state.resource_generation);
        state.prepared_instance_buffer_bytes = 0;
        state.in_flight_instance_buffer_bytes = 0;
        state.untracked_instance_buffer_bytes = 0;
        state.gpu_resources.clear_current();
    }

    pub(crate) fn begin_frame_submission(
        &self,
        frame_index: u64,
        device_epoch: u64,
        started_at: Instant,
    ) -> Option<WgpuFrameCompletionToken> {
        let mut state = self.lock();
        if !state.gpu_resources.tracking_enabled {
            let instance_buffer_bytes = state.prepared_instance_buffer_bytes;
            retain_untracked_submission(&mut state, instance_buffer_bytes);
            return None;
        }
        let instance_buffer_bytes = state.prepared_instance_buffer_bytes;
        if state.completion.device_epoch != device_epoch {
            state.completion.lost_samples = state.completion.lost_samples.saturating_add(1);
            retain_untracked_submission(&mut state, instance_buffer_bytes);
            return None;
        }
        if state.gpu_resources.pending_retirements >= state.config.max_pending_frame_completions() {
            state.completion.lost_samples = state.completion.lost_samples.saturating_add(1);
            retain_untracked_submission(&mut state, instance_buffer_bytes);
            return None;
        }

        state.prepared_instance_buffer_bytes = 0;
        state.in_flight_instance_buffer_bytes = state
            .in_flight_instance_buffer_bytes
            .saturating_add(instance_buffer_bytes);
        state.gpu_resources.pending_retirements += 1;
        let completion_generation = if state.completion.enabled
            && state.completion.pending_samples < state.config.max_pending_frame_completions()
        {
            state.completion.pending_samples += 1;
            Some(state.completion_generation)
        } else {
            state.completion.lost_samples = state.completion.lost_samples.saturating_add(1);
            None
        };
        refresh_gpu_resources(&mut state);
        Some(WgpuFrameCompletionToken {
            shared: Arc::clone(&self.shared),
            completion_generation,
            resource_generation: state.resource_generation,
            frame_index,
            device_epoch,
            started_at,
            instance_buffer_bytes,
            active: true,
        })
    }

    pub(crate) fn completion_stats(&self) -> WgpuFrameCompletionStats {
        self.lock().completion
    }

    pub(crate) fn drain_completed_frames(&self) -> Vec<WgpuFrameCompletionSample> {
        let mut state = self.lock();
        let samples = state.completed_frames.drain(..).collect::<Vec<_>>();
        state.completion.buffered_samples = 0;
        samples
    }

    pub(crate) fn observe_gpu_resources(&self, texture_bytes: u64, instance_buffer_bytes: u64) {
        let mut state = self.lock();
        state.prepared_instance_buffer_bytes = instance_buffer_bytes;
        let concurrent_instance_bytes = state
            .in_flight_instance_buffer_bytes
            .saturating_add(instance_buffer_bytes)
            .saturating_add(state.untracked_instance_buffer_bytes);
        state
            .gpu_resources
            .observe(texture_bytes, concurrent_instance_bytes);
    }

    pub(crate) fn finish_prepared_gpu_resources(&self) {
        let mut state = self.lock();
        state.prepared_instance_buffer_bytes = 0;
        refresh_gpu_resources(&mut state);
    }

    pub(crate) fn gpu_resource_stats(&self) -> WgpuGpuResourceStats {
        self.lock().gpu_resources
    }

    fn lock(&self) -> MutexGuard<'_, WgpuTelemetryState> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub(crate) struct WgpuFrameCompletionToken {
    shared: Arc<Mutex<WgpuTelemetryState>>,
    completion_generation: Option<u64>,
    resource_generation: u64,
    frame_index: u64,
    device_epoch: u64,
    started_at: Instant,
    instance_buffer_bytes: u64,
    active: bool,
}

impl WgpuFrameCompletionToken {
    pub(crate) fn complete(mut self) {
        let duration = self.started_at.elapsed();
        self.complete_inner(duration);
        self.active = false;
    }

    #[cfg(test)]
    fn complete_with_duration(mut self, duration: std::time::Duration) {
        self.complete_inner(duration);
        self.active = false;
    }

    fn complete_inner(&self, duration: Duration) {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if resource_token_is_current(&state, self) && state.gpu_resources.pending_retirements > 0 {
            state.gpu_resources.pending_retirements -= 1;
            state.in_flight_instance_buffer_bytes = state
                .in_flight_instance_buffer_bytes
                .saturating_sub(self.instance_buffer_bytes);
            refresh_gpu_resources(&mut state);
        }

        let Some(completion_generation) = self.completion_generation else {
            return;
        };
        if !state.completion.enabled
            || state.completion.device_epoch != self.device_epoch
            || state.completion_generation != completion_generation
            || state.completion.pending_samples == 0
        {
            return;
        }

        state.completion.pending_samples -= 1;
        state.completion.completed_samples = state.completion.completed_samples.saturating_add(1);
        if state.completed_frames.len()
            >= usize::try_from(state.config.max_buffered_frame_completions()).unwrap_or(usize::MAX)
        {
            state.completed_frames.pop_front();
            state.completion.lost_samples = state.completion.lost_samples.saturating_add(1);
        }
        state
            .completed_frames
            .push_back(WgpuFrameCompletionSample::new(
                self.frame_index,
                self.device_epoch,
                u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
            ));
        state.completion.buffered_samples =
            u32::try_from(state.completed_frames.len()).unwrap_or(u32::MAX);
    }
}

impl Drop for WgpuFrameCompletionToken {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if resource_token_is_current(&state, self) && state.gpu_resources.pending_retirements > 0 {
            state.gpu_resources.pending_retirements -= 1;
            state.in_flight_instance_buffer_bytes = state
                .in_flight_instance_buffer_bytes
                .saturating_sub(self.instance_buffer_bytes);
            if self.instance_buffer_bytes > 0 {
                state.untracked_instance_buffer_bytes = state
                    .untracked_instance_buffer_bytes
                    .saturating_add(self.instance_buffer_bytes);
                state.gpu_resources.lost_retirements =
                    state.gpu_resources.lost_retirements.saturating_add(1);
            }
            refresh_gpu_resources(&mut state);
        }

        if self.completion_generation.is_some_and(|generation| {
            state.completion.enabled
                && state.completion.device_epoch == self.device_epoch
                && state.completion_generation == generation
                && state.completion.pending_samples > 0
        }) {
            state.completion.pending_samples -= 1;
            state.completion.lost_samples = state.completion.lost_samples.saturating_add(1);
        }
    }
}

fn resource_token_is_current(state: &WgpuTelemetryState, token: &WgpuFrameCompletionToken) -> bool {
    state.completion.device_epoch == token.device_epoch
        && state.resource_generation == token.resource_generation
}

fn retain_untracked_submission(state: &mut WgpuTelemetryState, instance_buffer_bytes: u64) {
    state.prepared_instance_buffer_bytes = 0;
    if instance_buffer_bytes > 0 {
        state.untracked_instance_buffer_bytes = state
            .untracked_instance_buffer_bytes
            .saturating_add(instance_buffer_bytes);
        state.gpu_resources.lost_retirements =
            state.gpu_resources.lost_retirements.saturating_add(1);
    }
    refresh_gpu_resources(state);
}

fn refresh_gpu_resources(state: &mut WgpuTelemetryState) {
    let texture_bytes = state.gpu_resources.current_texture_bytes;
    let instance_buffer_bytes = state
        .in_flight_instance_buffer_bytes
        .saturating_add(state.prepared_instance_buffer_bytes)
        .saturating_add(state.untracked_instance_buffer_bytes);
    state
        .gpu_resources
        .observe(texture_bytes, instance_buffer_bytes);
}

fn next_generation(current: u64) -> u64 {
    current
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn frame_completion_tracking_bounds_pending_and_completed_samples() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.begin_device_epoch(7);
        let started = Instant::now();

        let first = telemetry
            .begin_frame_submission(10, 7, started)
            .expect("the first completion must be admitted");
        let second = telemetry
            .begin_frame_submission(11, 7, started)
            .expect("the second completion must be admitted");
        assert!(telemetry.begin_frame_submission(12, 7, started).is_none());

        first.complete_with_duration(Duration::from_nanos(10));
        second.complete_with_duration(Duration::from_nanos(11));
        let third = telemetry
            .begin_frame_submission(13, 7, started)
            .expect("capacity must return after completion");
        third.complete_with_duration(Duration::from_nanos(13));

        let stats = telemetry.completion_stats();
        assert_eq!(stats.pending_samples(), 0);
        assert_eq!(stats.buffered_samples(), 2);
        assert_eq!(stats.completed_samples(), 3);
        assert_eq!(stats.lost_samples(), 2);
        let samples = telemetry.drain_completed_frames();
        assert_eq!(
            samples
                .iter()
                .map(|sample| (
                    sample.frame_index(),
                    sample.device_epoch(),
                    sample.duration_ns()
                ))
                .collect::<Vec<_>>(),
            vec![(11, 7, 11), (13, 7, 13)]
        );
    }

    #[test]
    fn device_epoch_transition_quarantines_old_pending_completions() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.begin_device_epoch(7);
        let obsolete = telemetry
            .begin_frame_submission(10, 7, Instant::now())
            .unwrap();

        telemetry.begin_device_epoch(8);
        obsolete.complete_with_duration(Duration::from_nanos(10));
        let current = telemetry
            .begin_frame_submission(11, 8, Instant::now())
            .unwrap();
        current.complete_with_duration(Duration::from_nanos(11));

        let stats = telemetry.completion_stats();
        assert_eq!(stats.device_epoch(), 8);
        assert_eq!(stats.pending_samples(), 0);
        assert_eq!(stats.completed_samples(), 1);
        assert_eq!(stats.lost_samples(), 1);
        assert_eq!(
            telemetry.drain_completed_frames(),
            vec![WgpuFrameCompletionSample::new(11, 8, 11)]
        );
    }

    #[test]
    fn device_retirement_quarantines_same_epoch_completions() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.begin_device_epoch(7);
        telemetry.observe_gpu_resources(4, 64);
        let obsolete = telemetry
            .begin_frame_submission(10, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();

        telemetry.retire_device_epoch();
        obsolete.complete_with_duration(Duration::from_nanos(10));

        let stats = telemetry.completion_stats();
        assert_eq!(stats.device_epoch(), 7);
        assert_eq!(stats.pending_samples(), 0);
        assert_eq!(stats.completed_samples(), 0);
        assert_eq!(stats.lost_samples(), 1);
        let resources = telemetry.gpu_resource_stats();
        assert_eq!(resources.current_total_bytes(), 0);
        assert_eq!(resources.pending_retirements(), 0);
        assert_eq!(resources.lost_retirements(), 1);
    }

    #[test]
    fn dropped_completion_releases_pending_and_in_flight_bytes_as_lost() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.begin_device_epoch(7);
        telemetry.observe_gpu_resources(4, 64);
        let dropped = telemetry
            .begin_frame_submission(10, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();

        drop(dropped);

        let completion = telemetry.completion_stats();
        assert_eq!(completion.pending_samples(), 0);
        assert_eq!(completion.completed_samples(), 0);
        assert_eq!(completion.lost_samples(), 1);
        let resources = telemetry.gpu_resource_stats();
        assert_eq!(resources.current_texture_bytes(), 4);
        assert_eq!(resources.current_instance_buffer_bytes(), 64);
        assert_eq!(resources.peak_total_bytes(), 68);
        assert_eq!(resources.pending_retirements(), 0);
        assert_eq!(resources.lost_retirements(), 1);
    }

    #[test]
    fn same_epoch_reconfiguration_cannot_consume_a_new_pending_slot() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.begin_device_epoch(7);
        telemetry.observe_gpu_resources(4, 64);
        let obsolete = telemetry
            .begin_frame_submission(10, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();

        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.observe_gpu_resources(8, 32);
        let current = telemetry
            .begin_frame_submission(11, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();
        let overlapping = telemetry.gpu_resource_stats();
        assert_eq!(overlapping.current_instance_buffer_bytes(), 96);
        assert_eq!(overlapping.peak_total_bytes(), 104);
        assert_eq!(overlapping.pending_retirements(), 2);

        obsolete.complete_with_duration(Duration::from_nanos(10));
        assert_eq!(
            telemetry
                .gpu_resource_stats()
                .current_instance_buffer_bytes(),
            32
        );
        current.complete_with_duration(Duration::from_nanos(11));

        let stats = telemetry.completion_stats();
        assert_eq!(stats.pending_samples(), 0);
        assert_eq!(stats.completed_samples(), 1);
        assert_eq!(stats.lost_samples(), 1);
        assert_eq!(
            telemetry.drain_completed_frames(),
            vec![WgpuFrameCompletionSample::new(11, 7, 11)]
        );
        let resources = telemetry.gpu_resource_stats();
        assert_eq!(resources.current_instance_buffer_bytes(), 0);
        assert_eq!(resources.pending_retirements(), 0);
        assert_eq!(resources.lost_retirements(), 0);
    }

    #[test]
    fn gpu_resource_bytes_track_current_and_concurrent_peak_per_epoch() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        telemetry.begin_device_epoch(7);

        telemetry.observe_gpu_resources(4, 64);
        let first = telemetry
            .begin_frame_submission(10, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();
        telemetry.observe_gpu_resources(8, 32);
        let second = telemetry
            .begin_frame_submission(11, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();
        let stats = telemetry.gpu_resource_stats();
        assert_eq!(stats.device_epoch(), 7);
        assert!(stats.tracking_enabled());
        assert_eq!(stats.pending_retirements(), 2);
        assert_eq!(stats.lost_retirements(), 0);
        assert_eq!(stats.current_texture_bytes(), 8);
        assert_eq!(stats.current_instance_buffer_bytes(), 96);
        assert_eq!(stats.current_total_bytes(), 104);
        assert_eq!(stats.peak_texture_bytes(), 8);
        assert_eq!(stats.peak_instance_buffer_bytes(), 96);
        assert_eq!(stats.peak_total_bytes(), 104);

        first.complete_with_duration(Duration::from_nanos(10));
        assert_eq!(
            telemetry
                .gpu_resource_stats()
                .current_instance_buffer_bytes(),
            32
        );
        second.complete_with_duration(Duration::from_nanos(11));
        assert_eq!(
            telemetry
                .gpu_resource_stats()
                .current_instance_buffer_bytes(),
            0
        );

        telemetry.retire_device_epoch();
        let cleared = telemetry.gpu_resource_stats();
        assert_eq!(cleared.current_total_bytes(), 0);
        assert_eq!(cleared.peak_total_bytes(), 104);

        telemetry.begin_device_epoch(8);
        assert_eq!(
            telemetry.gpu_resource_stats(),
            WgpuGpuResourceStats::for_device_epoch(8, true)
        );
    }

    #[test]
    fn enabling_after_an_untracked_submission_cannot_claim_an_exact_resource_peak() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.begin_device_epoch(7);
        telemetry.observe_gpu_resources(4, 64);

        assert!(
            telemetry
                .begin_frame_submission(10, 7, Instant::now())
                .is_none()
        );
        telemetry.finish_prepared_gpu_resources();
        telemetry.configure(WgpuBackendTelemetryConfig::new(2, 2).unwrap());

        let resources = telemetry.gpu_resource_stats();
        assert!(resources.tracking_enabled());
        assert_eq!(resources.current_instance_buffer_bytes(), 64);
        assert_eq!(resources.peak_total_bytes(), 68);
        assert_eq!(resources.pending_retirements(), 0);
        assert_eq!(resources.lost_retirements(), 1);
    }

    #[test]
    fn callback_capacity_exhaustion_retains_an_uncertified_upper_bound() {
        let telemetry = WgpuBackendTelemetry::default();
        telemetry.configure(WgpuBackendTelemetryConfig::new(1, 2).unwrap());
        telemetry.begin_device_epoch(7);
        telemetry.observe_gpu_resources(4, 64);
        let first = telemetry
            .begin_frame_submission(10, 7, Instant::now())
            .unwrap();
        telemetry.finish_prepared_gpu_resources();

        telemetry.observe_gpu_resources(8, 32);
        assert!(
            telemetry
                .begin_frame_submission(11, 7, Instant::now())
                .is_none()
        );
        telemetry.finish_prepared_gpu_resources();
        let saturated = telemetry.gpu_resource_stats();
        assert_eq!(saturated.current_instance_buffer_bytes(), 96);
        assert_eq!(saturated.peak_total_bytes(), 104);
        assert_eq!(saturated.pending_retirements(), 1);
        assert_eq!(saturated.lost_retirements(), 1);

        first.complete_with_duration(Duration::from_nanos(10));
        let retired = telemetry.gpu_resource_stats();
        assert_eq!(retired.current_instance_buffer_bytes(), 32);
        assert_eq!(retired.pending_retirements(), 0);
        assert_eq!(retired.lost_retirements(), 1);
    }

    #[test]
    fn telemetry_configuration_rejects_zero_or_unbounded_capacity() {
        assert_eq!(
            WgpuBackendTelemetryConfig::new(0, 1),
            Err(WgpuTelemetryConfigError::ZeroPendingCapacity)
        );
        assert_eq!(
            WgpuBackendTelemetryConfig::new(1, 0),
            Err(WgpuTelemetryConfigError::ZeroCompletedCapacity)
        );
        assert!(
            WgpuBackendTelemetryConfig::new(
                MAX_PENDING_FRAME_COMPLETIONS + 1,
                MAX_BUFFERED_FRAME_COMPLETIONS,
            )
            .is_err()
        );
        assert!(
            WgpuBackendTelemetryConfig::new(
                MAX_PENDING_FRAME_COMPLETIONS,
                MAX_BUFFERED_FRAME_COMPLETIONS + 1,
            )
            .is_err()
        );
    }

    #[test]
    fn adapter_summary_bounds_and_normalizes_dynamic_driver_text() {
        let summary = WgpuAdapterSummary::from_parts(
            "adapter\nname",
            0x1234,
            0x5678,
            "discrete-gpu",
            "vulkan",
            &"d".repeat(MAX_ADAPTER_SUMMARY_FIELD_BYTES + 1),
            "driver\r\ninfo",
        );

        assert_eq!(summary.name(), "adapter name");
        assert_eq!(summary.vendor(), 0x1234);
        assert_eq!(summary.device(), 0x5678);
        assert_eq!(summary.device_type(), "discrete-gpu");
        assert_eq!(summary.backend(), "vulkan");
        assert_eq!(summary.driver().len(), MAX_ADAPTER_SUMMARY_FIELD_BYTES);
        assert_eq!(summary.driver_info(), "driver  info");
        assert!(
            summary
                .name()
                .chars()
                .chain(summary.driver().chars())
                .chain(summary.driver_info().chars())
                .all(|character| !character.is_control())
        );
    }
}
