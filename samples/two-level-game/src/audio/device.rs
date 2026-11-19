use std::{
    collections::VecDeque,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};

use super::{
    clips::{PreparedClips, SAMPLE_RATE},
    mixer::{Counters, Cue, EventKind, Mixer, OUTPUT_CHANNELS, Publication, Refusal, Scope},
};

const COMMAND_CAPACITY: usize = 64;
const CUE_RECORD_CAPACITY: usize = 12_288;
const SCOPE_RECORD_CAPACITY: usize = 256;
const DRIVER_RECORD_CAPACITY: usize = 256;
const CAPTURE_SAMPLES: usize = 512;
const CAPTURE_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum CaptureMode {
    #[default]
    Disabled = 0,
    Strict = 1,
    #[cfg(any(feature = "desktop", test))]
    BestEffort = 2,
}

#[derive(Clone, Debug, Default)]
pub(super) struct DeviceSettings {
    pub capture: CaptureMode,
    pub native_gate: Option<NativeGate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum SubmissionRefusal {
    #[error("audio playback has not been admitted")]
    NotStarted,
    #[error("audio admission is closed")]
    Closed,
    #[error("audio admission is closed after a fault")]
    Faulted,
    #[error("audio command queue is full")]
    QueueFull,
    #[error("audio observation capacity is reserved")]
    RecordCapacity,
    #[error("audio mixer refused the cue or scope: {0:?}")]
    Mixer(Refusal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CueRecordKind {
    Accepted,
    Duplicate,
    Started,
    Completed,
    Cancelled { started: bool },
    Refused(SubmissionRefusal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceRecord {
    Cue {
        cue: Cue,
        frame: u64,
        kind: CueRecordKind,
    },
    ScopeApplied {
        scope: Scope,
        frame: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CapturedOutput {
    pub first_frame: u64,
    pub first_channel: u16,
    pub channels: u16,
    pub len: usize,
    pub samples: [f32; CAPTURE_SAMPLES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum DeviceFault {
    NativeDeviceBusy,
    NativeDeviceChanged,
    NativeDeviceUnavailable,
    NativeHostUnavailable,
    NativeInvalidInput,
    NativePermissionDenied,
    NativeRealtimeDenied,
    NativeResourceExhausted,
    NativeStreamInvalidated,
    NativeUnsupportedConfig,
    NativeUnsupportedOperation,
    NativeXrun,
    NativeBackend,
    NativeOther,
    CallbackPanicked,
    NativeOperationPanicked,
    StreamDropPanicked,
    CallbackReleaseMismatch,
    RecordOverflow,
    CaptureOverflow,
    DriverLockPoisoned,
    MixerContract,
    FrameOverflow,
    NativeGateTimedOut,
    UnexpectedCallbackRelease,
    NativeBuildRetirementUnknown,
}

impl DeviceFault {
    pub(super) const fn mask(self) -> u64 {
        1_u64 << self as u8
    }

    fn native(error: &cpal::Error) -> Self {
        match error.kind() {
            cpal::ErrorKind::DeviceBusy => Self::NativeDeviceBusy,
            cpal::ErrorKind::DeviceChanged => Self::NativeDeviceChanged,
            cpal::ErrorKind::DeviceNotAvailable => Self::NativeDeviceUnavailable,
            cpal::ErrorKind::HostUnavailable => Self::NativeHostUnavailable,
            cpal::ErrorKind::InvalidInput => Self::NativeInvalidInput,
            cpal::ErrorKind::PermissionDenied => Self::NativePermissionDenied,
            cpal::ErrorKind::RealtimeDenied => Self::NativeRealtimeDenied,
            cpal::ErrorKind::ResourceExhausted => Self::NativeResourceExhausted,
            cpal::ErrorKind::StreamInvalidated => Self::NativeStreamInvalidated,
            cpal::ErrorKind::UnsupportedConfig => Self::NativeUnsupportedConfig,
            cpal::ErrorKind::UnsupportedOperation => Self::NativeUnsupportedOperation,
            cpal::ErrorKind::Xrun => Self::NativeXrun,
            cpal::ErrorKind::BackendError => Self::NativeBackend,
            _ => Self::NativeOther,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DeviceSnapshot {
    pub runtime: u64,
    pub desired_instance: u64,
    pub applied_instance: u64,
    pub play_admitted: bool,
    pub admission_open: bool,
    pub quiescing: bool,
    pub callback_count: u64,
    pub output_frame: u64,
    pub counters: Counters,
    pub faults: u64,
    pub stream_created: bool,
    pub stream_drop_returned: bool,
    pub data_callback_released: bool,
    pub error_callback_released: bool,
    pub physical_complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CloseReport {
    pub complete: bool,
    pub faults: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("{stage}: {detail}")]
pub(super) struct DeviceError {
    pub stage: &'static str,
    pub detail: String,
    pub retirement: DeviceSnapshot,
    pub witness: Option<AudioWitness>,
    pub retained_device: Option<AudioDevice>,
}

#[derive(Clone, Debug)]
pub(super) struct NativeGate {
    inner: Arc<GateState>,
}

#[derive(Debug)]
struct GateState {
    state: AtomicU8,
    timeout: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum GateError {
    #[error("native callback gate timeout must be within (0, 10 seconds]")]
    InvalidTimeout,
    #[error("native callback gate is already armed, entered, or timed out")]
    Unavailable,
}

impl NativeGate {
    pub(super) fn new(timeout: Duration) -> Result<Self, GateError> {
        if timeout.is_zero() || timeout > Duration::from_secs(10) {
            return Err(GateError::InvalidTimeout);
        }
        Ok(Self {
            inner: Arc::new(GateState {
                state: AtomicU8::new(0),
                timeout,
            }),
        })
    }

    pub(super) fn arm(&self) -> Result<(), GateError> {
        self.inner
            .state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| GateError::Unavailable)
    }

    pub(super) fn entered(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) == 2
    }

    pub(super) fn timed_out(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) == 4
    }

    pub(super) fn release(&self) -> bool {
        self.inner
            .state
            .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn wait_in_callback(&self) -> bool {
        if self
            .inner
            .state
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return true;
        }
        let entered = Instant::now();
        while self.inner.state.load(Ordering::Acquire) == 2 {
            if entered.elapsed() >= self.inner.timeout {
                if self
                    .inner
                    .state
                    .compare_exchange(2, 4, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return false;
                }
                break;
            }
            std::thread::yield_now();
        }
        true
    }
}

#[derive(Debug)]
struct Shared {
    runtime: u64,
    desired_instance: AtomicU64,
    applied_instance: AtomicU64,
    admission_open: AtomicBool,
    play_admitted: AtomicBool,
    quiescing: AtomicBool,
    callback_count: AtomicU64,
    output_frame: AtomicU64,
    dropped_output_frames: AtomicU64,
    accepted: AtomicU64,
    started: AtomicU64,
    completed: AtomicU64,
    cancelled: AtomicU64,
    faults: AtomicU64,
    cue_record_slots: AtomicUsize,
    scope_records: AtomicUsize,
    build_in_progress: AtomicBool,
    stream_created: AtomicBool,
    stream_drop_returned: AtomicBool,
    data_callback_released: AtomicBool,
    error_callback_released: AtomicBool,
    driver_records: Mutex<VecDeque<DeviceRecord>>,
}

impl Shared {
    fn new(runtime: u64) -> Self {
        Self {
            runtime,
            desired_instance: AtomicU64::new(0),
            applied_instance: AtomicU64::new(0),
            admission_open: AtomicBool::new(false),
            play_admitted: AtomicBool::new(false),
            quiescing: AtomicBool::new(false),
            callback_count: AtomicU64::new(0),
            output_frame: AtomicU64::new(0),
            dropped_output_frames: AtomicU64::new(0),
            accepted: AtomicU64::new(0),
            started: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            cancelled: AtomicU64::new(0),
            faults: AtomicU64::new(0),
            cue_record_slots: AtomicUsize::new(CUE_RECORD_CAPACITY),
            scope_records: AtomicUsize::new(0),
            build_in_progress: AtomicBool::new(false),
            stream_created: AtomicBool::new(false),
            stream_drop_returned: AtomicBool::new(false),
            data_callback_released: AtomicBool::new(true),
            error_callback_released: AtomicBool::new(true),
            driver_records: Mutex::new(VecDeque::with_capacity(DRIVER_RECORD_CAPACITY)),
        }
    }

    fn fault(&self, fault: DeviceFault) {
        self.faults.fetch_or(fault.mask(), Ordering::AcqRel);
        self.admission_open.store(false, Ordering::Release);
    }

    fn refusal(&self) -> Option<SubmissionRefusal> {
        if self.quiescing.load(Ordering::Acquire) {
            Some(SubmissionRefusal::Closed)
        } else if self.faults.load(Ordering::Acquire) != 0 {
            Some(SubmissionRefusal::Faulted)
        } else if !self.admission_open.load(Ordering::Acquire) {
            Some(SubmissionRefusal::NotStarted)
        } else {
            None
        }
    }

    fn driver_refusal(&self, cue: Cue, reason: SubmissionRefusal) {
        let mut records = self.driver_records.lock().unwrap_or_else(|poisoned| {
            self.fault(DeviceFault::DriverLockPoisoned);
            poisoned.into_inner()
        });
        if records.len() == DRIVER_RECORD_CAPACITY {
            self.fault(DeviceFault::RecordOverflow);
        } else {
            records.push_back(DeviceRecord::Cue {
                cue,
                frame: self.output_frame.load(Ordering::Acquire),
                kind: CueRecordKind::Refused(reason),
            });
        }
    }

    fn snapshot(&self) -> DeviceSnapshot {
        let stream_drop_returned = self.stream_drop_returned.load(Ordering::Acquire);
        let data_callback_released = self.data_callback_released.load(Ordering::Acquire);
        let error_callback_released = self.error_callback_released.load(Ordering::Acquire);
        let faults = self.faults.load(Ordering::Acquire);
        let quiescing = self.quiescing.load(Ordering::Acquire);
        let stream_created = self.stream_created.load(Ordering::Acquire);
        DeviceSnapshot {
            runtime: self.runtime,
            desired_instance: self.desired_instance.load(Ordering::Acquire),
            applied_instance: self.applied_instance.load(Ordering::Acquire),
            play_admitted: self.play_admitted.load(Ordering::Acquire),
            admission_open: self.admission_open.load(Ordering::Acquire)
                && faults == 0
                && !quiescing,
            quiescing,
            callback_count: self.callback_count.load(Ordering::Acquire),
            output_frame: self.output_frame.load(Ordering::Acquire),
            counters: Counters {
                accepted: self.accepted.load(Ordering::Acquire),
                started: self.started.load(Ordering::Acquire),
                completed: self.completed.load(Ordering::Acquire),
                cancelled: self.cancelled.load(Ordering::Acquire),
            },
            faults,
            stream_created,
            stream_drop_returned,
            data_callback_released,
            error_callback_released,
            physical_complete: quiescing
                && (!stream_created || stream_drop_returned)
                && data_callback_released
                && error_callback_released
                && faults
                    & (DeviceFault::StreamDropPanicked.mask()
                        | DeviceFault::CallbackReleaseMismatch.mask()
                        | DeviceFault::NativeBuildRetirementUnknown.mask())
                    == 0,
        }
    }

    fn publish_counters(&self, mixer: &Mixer) {
        let counters = mixer.counters();
        self.accepted.store(counters.accepted, Ordering::Release);
        self.started.store(counters.started, Ordering::Release);
        self.completed.store(counters.completed, Ordering::Release);
        self.cancelled.store(counters.cancelled, Ordering::Release);
        self.output_frame
            .store(mixer.output_frame(), Ordering::Release);
    }
}

#[derive(Clone, Debug)]
pub(super) struct AudioController {
    shared: Arc<Shared>,
    commands: Arc<Mutex<Producer<Cue>>>,
}

impl AudioController {
    pub(super) fn set_scope(&self, scope: Scope) -> Result<(), SubmissionRefusal> {
        let commands = self.commands.lock().map_err(|_| {
            self.shared.fault(DeviceFault::DriverLockPoisoned);
            SubmissionRefusal::Faulted
        })?;
        if let Some(reason) = self.shared.refusal() {
            return Err(reason);
        }
        if commands.is_abandoned() {
            return Err(SubmissionRefusal::Closed);
        }
        if scope.runtime == 0 || scope.instance == 0 {
            return Err(SubmissionRefusal::Mixer(Refusal::InvalidScope));
        }
        if scope.runtime != self.shared.runtime {
            return Err(SubmissionRefusal::Mixer(Refusal::ForeignRuntime));
        }
        if scope.instance < self.shared.desired_instance.load(Ordering::Acquire) {
            return Err(SubmissionRefusal::Mixer(Refusal::StaleScope));
        }
        self.shared
            .desired_instance
            .store(scope.instance, Ordering::Release);
        Ok(())
    }

    pub(super) fn submit(&self, cue: Cue) -> Result<(), SubmissionRefusal> {
        let result = self.submit_inner(cue);
        if let Err(reason) = result {
            self.shared.driver_refusal(cue, reason);
        }
        result
    }

    fn submit_inner(&self, cue: Cue) -> Result<(), SubmissionRefusal> {
        let mut commands = self.commands.lock().map_err(|_| {
            self.shared.fault(DeviceFault::DriverLockPoisoned);
            SubmissionRefusal::Faulted
        })?;
        if let Some(reason) = self.shared.refusal() {
            return Err(reason);
        }
        if commands.is_abandoned() {
            return Err(SubmissionRefusal::Closed);
        }
        if cue.id.scope.runtime != self.shared.runtime {
            return Err(SubmissionRefusal::Mixer(Refusal::ForeignRuntime));
        }
        if cue.id.scope.instance != self.shared.desired_instance.load(Ordering::Acquire) {
            return Err(SubmissionRefusal::Mixer(Refusal::ForeignScope));
        }
        if commands.slots() == 0 {
            return Err(SubmissionRefusal::QueueFull);
        }
        if self.shared.cue_record_slots.load(Ordering::Acquire) < 3 {
            return Err(SubmissionRefusal::RecordCapacity);
        }
        self.shared.cue_record_slots.fetch_sub(3, Ordering::AcqRel);
        if commands.push(cue).is_err() {
            self.shared.cue_record_slots.fetch_add(3, Ordering::Release);
            return Err(SubmissionRefusal::QueueFull);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Observations {
    records: Consumer<DeviceRecord>,
    capture: Option<Consumer<CapturedOutput>>,
}

#[derive(Clone, Debug)]
pub(super) struct AudioWitness {
    shared: Arc<Shared>,
    observations: Arc<Mutex<Observations>>,
}

impl AudioWitness {
    pub(super) fn snapshot(&self) -> DeviceSnapshot {
        self.shared.snapshot()
    }

    pub(super) fn drain_records(&self) -> Vec<DeviceRecord> {
        let mut observations = self.observations.lock().unwrap_or_else(|poisoned| {
            self.shared.fault(DeviceFault::DriverLockPoisoned);
            poisoned.into_inner()
        });
        let mut records = Vec::new();
        for _ in 0..CUE_RECORD_CAPACITY + SCOPE_RECORD_CAPACITY {
            let Ok(record) = observations.records.pop() else {
                break;
            };
            match record {
                DeviceRecord::Cue { .. } => {
                    self.shared.cue_record_slots.fetch_add(1, Ordering::Release);
                }
                DeviceRecord::ScopeApplied { .. } => {
                    self.shared.scope_records.fetch_sub(1, Ordering::Release);
                }
            }
            records.push(record);
        }
        let mut driver = self
            .shared
            .driver_records
            .lock()
            .unwrap_or_else(|poisoned| {
                self.shared.fault(DeviceFault::DriverLockPoisoned);
                poisoned.into_inner()
            });
        records.extend(driver.drain(..));
        records
    }

    pub(super) fn drain_capture(&self) -> Vec<CapturedOutput> {
        let mut observations = self.observations.lock().unwrap_or_else(|poisoned| {
            self.shared.fault(DeviceFault::DriverLockPoisoned);
            poisoned.into_inner()
        });
        let mut chunks = Vec::new();
        if let Some(capture) = &mut observations.capture {
            for _ in 0..CAPTURE_CAPACITY {
                let Ok(chunk) = capture.pop() else { break };
                chunks.push(chunk);
            }
        }
        chunks
    }

    pub(super) fn dropped_output_frames(&self) -> u64 {
        self.shared.dropped_output_frames.load(Ordering::Acquire)
    }
}

pub(super) struct AudioDevice {
    stream: Option<cpal::Stream>,
    control: AudioController,
    witness: AudioWitness,
    _driver_thread: PhantomData<Rc<()>>,
}

impl AudioDevice {
    pub(super) fn open(
        runtime: u64,
        clips: PreparedClips,
        settings: DeviceSettings,
    ) -> Result<Self, DeviceError> {
        let shared = Arc::new(Shared::new(runtime));
        let mut failure_witness = None;
        let result = catch_unwind(AssertUnwindSafe(|| {
            Self::build(shared.clone(), clips, settings, &mut failure_witness)
        }));
        let mut device = match result {
            Ok(Ok(device)) => device,
            Ok(Err((stage, detail))) => {
                return Err(Self::opening_error(
                    &shared,
                    stage,
                    detail,
                    failure_witness,
                    None,
                ));
            }
            Err(_) => {
                shared.fault(DeviceFault::NativeOperationPanicked);
                if shared.build_in_progress.load(Ordering::Acquire) {
                    shared.fault(DeviceFault::NativeBuildRetirementUnknown);
                }
                return Err(Self::opening_error(
                    &shared,
                    "build",
                    "native audio construction panicked".into(),
                    failure_witness,
                    None,
                ));
            }
        };
        let play = catch_unwind(AssertUnwindSafe(|| {
            device.stream.as_ref().map(StreamTrait::play)
        }));
        let detail = match play {
            Ok(Some(Ok(()))) => {
                shared.play_admitted.store(true, Ordering::Release);
                shared.admission_open.store(true, Ordering::Release);
                if shared.faults.load(Ordering::Acquire) != 0 {
                    shared.admission_open.store(false, Ordering::Release);
                }
                return Ok(device);
            }
            Ok(Some(Err(error))) => {
                shared.fault(DeviceFault::native(&error));
                error.to_string()
            }
            Ok(None) => {
                shared.fault(DeviceFault::NativeOperationPanicked);
                "constructed stream is missing".into()
            }
            Err(_) => {
                shared.fault(DeviceFault::NativeOperationPanicked);
                "native audio play admission panicked".into()
            }
        };
        let report = device.close();
        let retained_device = if report.complete { None } else { Some(device) };
        Err(Self::opening_error(
            &shared,
            "play",
            detail,
            failure_witness,
            retained_device,
        ))
    }

    fn opening_error(
        shared: &Shared,
        stage: &'static str,
        detail: String,
        witness: Option<AudioWitness>,
        retained_device: Option<AudioDevice>,
    ) -> DeviceError {
        shared.admission_open.store(false, Ordering::Release);
        shared.quiescing.store(true, Ordering::Release);
        if !shared.data_callback_released.load(Ordering::Acquire)
            || !shared.error_callback_released.load(Ordering::Acquire)
        {
            shared.fault(DeviceFault::CallbackReleaseMismatch);
        }
        DeviceError {
            stage,
            detail,
            retirement: shared.snapshot(),
            witness,
            retained_device,
        }
    }

    fn build(
        shared: Arc<Shared>,
        clips: PreparedClips,
        settings: DeviceSettings,
        failure_witness: &mut Option<AudioWitness>,
    ) -> Result<Self, (&'static str, String)> {
        if shared.runtime == 0 {
            return Err((
                "runtime",
                "audio requires a nonzero Runtime generation".into(),
            ));
        }
        let host = cpal::host_from_id(cpal::HostId::Wasapi)
            .map_err(|error| ("host", error.to_string()))?;
        let output = host
            .default_output_device()
            .ok_or_else(|| ("device", "no default WASAPI output device".into()))?;
        let config = output
            .supported_output_configs()
            .map_err(|error| ("configuration", error.to_string()))?
            .filter(|config| {
                config.channels() == OUTPUT_CHANNELS as u16
                    && config.sample_format() == cpal::SampleFormat::F32
            })
            .find_map(|config| config.try_with_sample_rate(SAMPLE_RATE))
            .ok_or_else(|| {
                (
                    "configuration",
                    "default output does not support stereo 48000 Hz f32".into(),
                )
            })?
            .config();
        let (commands, command_input) = RingBuffer::new(COMMAND_CAPACITY);
        let (record_output, records) = RingBuffer::new(CUE_RECORD_CAPACITY + SCOPE_RECORD_CAPACITY);
        let (capture_output, capture) = if settings.capture != CaptureMode::Disabled {
            let (producer, consumer) = RingBuffer::new(CAPTURE_CAPACITY);
            (Some((settings.capture, producer)), Some(consumer))
        } else {
            (None, None)
        };
        let control = AudioController {
            shared: shared.clone(),
            commands: Arc::new(Mutex::new(commands)),
        };
        let witness = AudioWitness {
            shared: shared.clone(),
            observations: Arc::new(Mutex::new(Observations { records, capture })),
        };
        *failure_witness = Some(witness.clone());
        let mut callback = CallbackState {
            mixer: Mixer::new(clips),
            admission: control.commands.clone(),
            commands: command_input,
            records: record_output,
            capture: capture_output,
            gate: settings.native_gate,
            release: ReleaseToken::new(shared.clone(), true),
        };
        let error_release = ReleaseToken::new(shared.clone(), false);
        shared.build_in_progress.store(true, Ordering::Release);
        let stream = output.build_output_stream::<f32, _, _>(
            config,
            move |output, _| callback.output(output),
            move |error| error_release.native_error(&error),
            Some(Duration::from_secs(3)),
        );
        shared.build_in_progress.store(false, Ordering::Release);
        let stream = stream.map_err(|error| ("build", error.to_string()))?;
        shared.stream_created.store(true, Ordering::Release);
        Ok(Self {
            stream: Some(stream),
            control,
            witness,
            _driver_thread: PhantomData,
        })
    }

    pub(super) fn control(&self) -> AudioController {
        self.control.clone()
    }

    pub(super) fn witness(&self) -> AudioWitness {
        self.witness.clone()
    }

    pub(super) fn quiesce(&self) {
        let shared = &self.control.shared;
        let _commands = self.control.commands.lock().unwrap_or_else(|poisoned| {
            shared.fault(DeviceFault::DriverLockPoisoned);
            poisoned.into_inner()
        });
        shared.admission_open.store(false, Ordering::Release);
        shared.quiescing.store(true, Ordering::Release);
    }

    pub(super) fn close(&mut self) -> CloseReport {
        self.quiesce();
        let shared = &self.control.shared;
        if let Some(stream) = self.stream.take() {
            if catch_unwind(AssertUnwindSafe(|| drop(stream))).is_ok() {
                shared.stream_drop_returned.store(true, Ordering::Release);
            } else {
                shared.fault(DeviceFault::StreamDropPanicked);
            }
            if !shared.data_callback_released.load(Ordering::Acquire)
                || !shared.error_callback_released.load(Ordering::Acquire)
            {
                shared.fault(DeviceFault::CallbackReleaseMismatch);
            }
        }
        let snapshot = shared.snapshot();
        CloseReport {
            complete: snapshot.physical_complete,
            faults: snapshot.faults,
        }
    }
}

impl std::fmt::Debug for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevice")
            .field("snapshot", &self.witness.snapshot())
            .finish_non_exhaustive()
    }
}

impl Drop for AudioDevice {
    fn drop(&mut self) {
        self.close();
    }
}

struct ReleaseToken {
    shared: Arc<Shared>,
    data: bool,
}

impl ReleaseToken {
    fn new(shared: Arc<Shared>, data: bool) -> Self {
        if data {
            shared
                .data_callback_released
                .store(false, Ordering::Release);
        } else {
            shared
                .error_callback_released
                .store(false, Ordering::Release);
        }
        Self { shared, data }
    }

    fn native_error(&self, error: &cpal::Error) {
        self.shared.fault(DeviceFault::native(error));
    }
}

impl Drop for ReleaseToken {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.shared.fault(DeviceFault::CallbackPanicked);
        }
        if self.shared.stream_created.load(Ordering::Acquire)
            && !self.shared.quiescing.load(Ordering::Acquire)
        {
            self.shared.fault(DeviceFault::UnexpectedCallbackRelease);
        }
        if self.data {
            self.shared
                .data_callback_released
                .store(true, Ordering::Release);
        } else {
            self.shared
                .error_callback_released
                .store(true, Ordering::Release);
        }
    }
}

struct CallbackState {
    mixer: Mixer,
    admission: Arc<Mutex<Producer<Cue>>>,
    commands: Consumer<Cue>,
    records: Producer<DeviceRecord>,
    capture: Option<(CaptureMode, Producer<CapturedOutput>)>,
    gate: Option<NativeGate>,
    release: ReleaseToken,
}

impl CallbackState {
    fn output(&mut self, output: &mut [f32]) {
        output.fill(0.0);
        if self.release.shared.faults.load(Ordering::Acquire) & DeviceFault::CallbackPanicked.mask()
            != 0
        {
            return;
        }
        if catch_unwind(AssertUnwindSafe(|| self.output_inner(output))).is_err() {
            self.release.shared.fault(DeviceFault::CallbackPanicked);
            output.fill(0.0);
        }
    }

    fn output_inner(&mut self, output: &mut [f32]) {
        let first_frame = self.mixer.output_frame();
        let shared = self.release.shared.clone();
        let callbacks = shared.callback_count.load(Ordering::Acquire);
        if let Some(next) = callbacks.checked_add(1) {
            shared.callback_count.store(next, Ordering::Release);
        } else {
            shared.fault(DeviceFault::FrameOverflow);
        }
        if let Some(gate) = &self.gate
            && !gate.wait_in_callback()
        {
            shared.fault(DeviceFault::NativeGateTimedOut);
        }
        if !output.len().is_multiple_of(OUTPUT_CHANNELS) {
            shared.fault(DeviceFault::MixerContract);
            return;
        }
        let Some(end_frame) = first_frame.checked_add((output.len() / OUTPUT_CHANNELS) as u64)
        else {
            shared.fault(DeviceFault::FrameOverflow);
            return;
        };
        if let Some((CaptureMode::Strict, capture)) = &self.capture
            && capture.slots() < output.len().div_ceil(CAPTURE_SAMPLES)
        {
            shared.fault(DeviceFault::CaptureOverflow);
        }
        self.apply_scope();
        for _ in 0..COMMAND_CAPACITY {
            let Ok(cue) = self.commands.pop() else { break };
            self.apply_scope();
            if let Some(reason) = shared.refusal() {
                self.refuse_queued(cue, reason);
                continue;
            }
            match self.mixer.publish(cue) {
                Ok(Publication::Accepted) => self.record(DeviceRecord::Cue {
                    cue,
                    frame: self.mixer.output_frame(),
                    kind: CueRecordKind::Accepted,
                }),
                Ok(Publication::Duplicate) => {
                    self.record(DeviceRecord::Cue {
                        cue,
                        frame: self.mixer.output_frame(),
                        kind: CueRecordKind::Duplicate,
                    });
                    shared.cue_record_slots.fetch_add(2, Ordering::Release);
                }
                Err(reason) => self.refuse_queued(cue, SubmissionRefusal::Mixer(reason)),
            }
        }
        for stereo in output.chunks_exact_mut(OUTPUT_CHANNELS) {
            self.apply_scope();
            if shared.quiescing.load(Ordering::Acquire)
                || shared.faults.load(Ordering::Acquire) != 0
            {
                self.mixer.cancel_all();
                self.flush_events();
            }
            if self.mixer.render(stereo).is_err() {
                shared.fault(DeviceFault::MixerContract);
                break;
            }
            self.flush_events();
        }
        shared.publish_counters(&self.mixer);
        if self.mixer.output_frame() != end_frame {
            shared.fault(DeviceFault::FrameOverflow);
        }
        if let Some((mode, capture)) = &mut self.capture {
            for (index, samples) in output.chunks(CAPTURE_SAMPLES).enumerate() {
                let chunk_frame = first_frame + (index * CAPTURE_SAMPLES / OUTPUT_CHANNELS) as u64;
                let mut chunk = CapturedOutput {
                    first_frame: chunk_frame,
                    first_channel: 0,
                    channels: OUTPUT_CHANNELS as u16,
                    len: samples.len(),
                    samples: [0.0; CAPTURE_SAMPLES],
                };
                chunk.samples[..samples.len()].copy_from_slice(samples);
                if capture.push(chunk).is_err() {
                    shared
                        .dropped_output_frames
                        .fetch_add(end_frame - chunk_frame, Ordering::Release);
                    if *mode == CaptureMode::Strict {
                        shared.fault(DeviceFault::CaptureOverflow);
                    }
                    break;
                }
            }
        }
    }

    fn apply_scope(&mut self) {
        let shared = &self.release.shared;
        let instance = shared.desired_instance.load(Ordering::Acquire);
        if instance == 0 || instance == shared.applied_instance.load(Ordering::Acquire) {
            return;
        }
        let scope = Scope {
            runtime: shared.runtime,
            instance,
        };
        if self.mixer.set_scope(scope).is_err() {
            shared.fault(DeviceFault::MixerContract);
            return;
        }
        self.flush_events();
        let shared = &self.release.shared;
        shared.applied_instance.store(instance, Ordering::Release);
        if shared.scope_records.load(Ordering::Acquire) == SCOPE_RECORD_CAPACITY {
            shared.fault(DeviceFault::RecordOverflow);
        } else {
            shared.scope_records.fetch_add(1, Ordering::Release);
            self.record(DeviceRecord::ScopeApplied {
                scope,
                frame: self.mixer.output_frame(),
            });
        }
    }

    fn refuse_queued(&mut self, cue: Cue, reason: SubmissionRefusal) {
        self.record(DeviceRecord::Cue {
            cue,
            frame: self.mixer.output_frame(),
            kind: CueRecordKind::Refused(reason),
        });
        self.release
            .shared
            .cue_record_slots
            .fetch_add(2, Ordering::Release);
    }

    fn flush_events(&mut self) {
        while let Some(event) = self.mixer.pop_event() {
            let kind = match event.kind {
                EventKind::Started => CueRecordKind::Started,
                EventKind::Completed => CueRecordKind::Completed,
                EventKind::Cancelled { started } => {
                    if !started {
                        self.release
                            .shared
                            .cue_record_slots
                            .fetch_add(1, Ordering::Release);
                    }
                    CueRecordKind::Cancelled { started }
                }
            };
            self.record(DeviceRecord::Cue {
                cue: event.cue,
                frame: event.frame,
                kind,
            });
        }
    }

    fn record(&mut self, record: DeviceRecord) {
        if self.records.push(record).is_err() {
            self.release.shared.fault(DeviceFault::RecordOverflow);
        }
    }

    fn retire(&mut self) {
        let shared = self.release.shared.clone();
        shared.admission_open.store(false, Ordering::Release);
        shared.quiescing.store(true, Ordering::Release);
        let admission = self.admission.clone();
        let _admission = admission.lock().unwrap_or_else(|poisoned| {
            shared.fault(DeviceFault::DriverLockPoisoned);
            poisoned.into_inner()
        });
        self.mixer.cancel_all();
        self.flush_events();
        for _ in 0..COMMAND_CAPACITY {
            let Ok(cue) = self.commands.pop() else { break };
            self.refuse_queued(cue, SubmissionRefusal::Closed);
        }
        self.release.shared.publish_counters(&self.mixer);
    }
}

impl Drop for CallbackState {
    fn drop(&mut self) {
        if catch_unwind(AssertUnwindSafe(|| self.retire())).is_err() {
            self.release.shared.fault(DeviceFault::CallbackPanicked);
        }
    }
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
