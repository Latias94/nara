use std::{
    collections::VecDeque,
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    mem::size_of,
    sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
    },
};

use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Resource;

use crate::AssetWatchEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetWatchQueueLimits {
    events: ItemLimit,
    retained_bytes: ByteLimit,
}

impl AssetWatchQueueLimits {
    #[must_use]
    pub const fn new(events: ItemLimit, retained_bytes: ByteLimit) -> Self {
        Self {
            events,
            retained_bytes,
        }
    }

    #[must_use]
    pub const fn events(self) -> ItemLimit {
        self.events
    }

    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }
}

impl Default for AssetWatchQueueLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(4_096).expect("default asset watch event limit is non-zero"),
            ByteLimit::new(4 * 1024 * 1024).expect("default asset watch byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetWatchQueueSendError {
    Full {
        requested_events: usize,
        maximum_events: usize,
        requested_bytes: usize,
        maximum_bytes: usize,
    },
    Busy,
    Disconnected,
    RescanRequired,
    Unavailable,
}

impl Display for AssetWatchQueueSendError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full {
                requested_events,
                maximum_events,
                requested_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "asset watch queue limit exceeded: {requested_events} events/{requested_bytes} bytes requested, maximum {maximum_events} events/{maximum_bytes} bytes"
            ),
            Self::Busy => formatter.write_str("asset watch queue producer is busy"),
            Self::Disconnected => formatter.write_str("asset watch queue receiver is disconnected"),
            Self::RescanRequired => {
                formatter.write_str("asset watch queue requires a source rescan")
            }
            Self::Unavailable => formatter.write_str("asset watch queue is unavailable"),
        }
    }
}

impl Error for AssetWatchQueueSendError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum AssetWatchFailureKind {
    Overflow = 0,
    Busy = 1,
    Disconnected = 2,
    Translation = 3,
    Backend = 4,
    Unavailable = 5,
    SourceAdmission = 6,
}

impl AssetWatchFailureKind {
    pub(crate) const ALL: [Self; 7] = [
        Self::Overflow,
        Self::Busy,
        Self::Disconnected,
        Self::Translation,
        Self::Backend,
        Self::Unavailable,
        Self::SourceAdmission,
    ];

    const COUNT: usize = Self::ALL.len();

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetWatchQueueStats {
    retained_events: usize,
    retained_bytes: usize,
    high_water_events: usize,
    high_water_bytes: usize,
    accepted_batches: u64,
    accepted_events: u64,
    failures: [u64; AssetWatchFailureKind::COUNT],
    suppressed_batches: u64,
    discarded_events: u64,
    rescan_required: bool,
}

impl AssetWatchQueueStats {
    #[must_use]
    pub const fn retained_events(self) -> usize {
        self.retained_events
    }

    #[must_use]
    pub const fn retained_bytes(self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub const fn high_water_events(self) -> usize {
        self.high_water_events
    }

    #[must_use]
    pub const fn high_water_bytes(self) -> usize {
        self.high_water_bytes
    }

    #[must_use]
    pub const fn accepted_batches(self) -> u64 {
        self.accepted_batches
    }

    #[must_use]
    pub const fn accepted_events(self) -> u64 {
        self.accepted_events
    }

    #[must_use]
    pub const fn overflow_rejections(self) -> u64 {
        self.failure(AssetWatchFailureKind::Overflow)
    }

    #[must_use]
    pub const fn busy_rejections(self) -> u64 {
        self.failure(AssetWatchFailureKind::Busy)
    }

    #[must_use]
    pub const fn disconnect_rejections(self) -> u64 {
        self.failure(AssetWatchFailureKind::Disconnected)
    }

    #[must_use]
    pub const fn translation_failures(self) -> u64 {
        self.failure(AssetWatchFailureKind::Translation)
    }

    #[must_use]
    pub const fn backend_failures(self) -> u64 {
        self.failure(AssetWatchFailureKind::Backend)
    }

    #[must_use]
    pub const fn unavailable_failures(self) -> u64 {
        self.failure(AssetWatchFailureKind::Unavailable)
    }

    #[must_use]
    pub const fn source_admission_failures(self) -> u64 {
        self.failure(AssetWatchFailureKind::SourceAdmission)
    }

    #[must_use]
    pub const fn suppressed_batches(self) -> u64 {
        self.suppressed_batches
    }

    #[must_use]
    pub const fn discarded_events(self) -> u64 {
        self.discarded_events
    }

    #[must_use]
    pub const fn rescan_required(self) -> bool {
        self.rescan_required
    }

    pub(crate) const fn failure(self, kind: AssetWatchFailureKind) -> u64 {
        self.failures[kind.index()]
    }
}

impl Default for AssetWatchQueueStats {
    fn default() -> Self {
        Self {
            retained_events: 0,
            retained_bytes: 0,
            high_water_events: 0,
            high_water_bytes: 0,
            accepted_batches: 0,
            accepted_events: 0,
            failures: [0; AssetWatchFailureKind::COUNT],
            suppressed_batches: 0,
            discarded_events: 0,
            rescan_required: false,
        }
    }
}

#[derive(Debug)]
struct AssetWatchEventBatch {
    sequence: u64,
    events: Vec<AssetWatchEvent>,
    retained_bytes: usize,
}

impl AssetWatchEventBatch {
    fn new(sequence: u64, mut events: Vec<AssetWatchEvent>) -> Self {
        events.shrink_to_fit();
        let retained_bytes = size_of::<Self>()
            .saturating_add(
                events
                    .capacity()
                    .saturating_mul(size_of::<AssetWatchEvent>()),
            )
            .saturating_add(
                events
                    .iter()
                    .map(AssetWatchEvent::retained_path_bytes)
                    .fold(0, usize::saturating_add),
            );
        Self {
            sequence,
            events,
            retained_bytes,
        }
    }
}

#[derive(Debug)]
struct AssetWatchQueueCounters {
    accepted_batches: AtomicU64,
    accepted_events: AtomicU64,
    failures: [AtomicU64; AssetWatchFailureKind::COUNT],
    suppressed_batches: AtomicU64,
    discarded_events: AtomicU64,
}

impl Default for AssetWatchQueueCounters {
    fn default() -> Self {
        Self {
            accepted_batches: AtomicU64::new(0),
            accepted_events: AtomicU64::new(0),
            failures: std::array::from_fn(|_| AtomicU64::new(0)),
            suppressed_batches: AtomicU64::new(0),
            discarded_events: AtomicU64::new(0),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AssetWatchQueueShared {
    limits: AssetWatchQueueLimits,
    next_sequence: Mutex<u64>,
    published_sequence: AtomicU64,
    receiver_connected: AtomicBool,
    rescan_required: AtomicBool,
    retained_events: AtomicUsize,
    retained_bytes: AtomicUsize,
    high_water_events: AtomicUsize,
    high_water_bytes: AtomicUsize,
    counters: AssetWatchQueueCounters,
}

impl AssetWatchQueueShared {
    fn new(limits: AssetWatchQueueLimits) -> Self {
        Self {
            limits,
            next_sequence: Mutex::new(1),
            published_sequence: AtomicU64::new(0),
            receiver_connected: AtomicBool::new(true),
            rescan_required: AtomicBool::new(false),
            retained_events: AtomicUsize::new(0),
            retained_bytes: AtomicUsize::new(0),
            high_water_events: AtomicUsize::new(0),
            high_water_bytes: AtomicUsize::new(0),
            counters: AssetWatchQueueCounters::default(),
        }
    }

    fn require_rescan(&self) {
        self.rescan_required.store(true, Ordering::Release);
    }

    fn record_failure(
        &self,
        kind: AssetWatchFailureKind,
        discarded_events: usize,
        require_rescan: bool,
    ) {
        saturating_increment(&self.counters.failures[kind.index()], 1);
        self.record_discarded(discarded_events);
        if require_rescan {
            self.require_rescan();
        }
    }

    pub(crate) fn record_translation_failure(&self, discarded_events: usize) {
        self.record_failure(AssetWatchFailureKind::Translation, discarded_events, true);
    }

    pub(crate) fn record_source_admission_failure(&self, discarded_events: usize) {
        self.record_failure(
            AssetWatchFailureKind::SourceAdmission,
            discarded_events,
            true,
        );
    }

    fn record_backend_failure(&self) {
        self.record_failure(AssetWatchFailureKind::Backend, 0, true);
    }

    fn record_unavailable(&self, discarded_events: usize) {
        self.record_failure(AssetWatchFailureKind::Unavailable, discarded_events, true);
    }

    fn record_discarded(&self, discarded_events: usize) {
        saturating_increment(
            &self.counters.discarded_events,
            usize_to_u64(discarded_events),
        );
    }

    fn reserve(counter: &AtomicUsize, amount: usize, maximum: usize) -> Result<usize, usize> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(amount)
                    .filter(|requested| *requested <= maximum)
            })
            .map(|previous| previous + amount)
            .map_err(|current| current.saturating_add(amount))
    }

    fn release(counter: &AtomicUsize, amount: usize) {
        let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(current.saturating_sub(amount))
        });
    }

    fn stats(&self) -> AssetWatchQueueStats {
        let load_u64 = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let failures = std::array::from_fn(|index| load_u64(&self.counters.failures[index]));
        AssetWatchQueueStats {
            retained_events: self.retained_events.load(Ordering::Acquire),
            retained_bytes: self.retained_bytes.load(Ordering::Acquire),
            high_water_events: self.high_water_events.load(Ordering::Acquire),
            high_water_bytes: self.high_water_bytes.load(Ordering::Acquire),
            accepted_batches: load_u64(&self.counters.accepted_batches),
            accepted_events: load_u64(&self.counters.accepted_events),
            failures,
            suppressed_batches: load_u64(&self.counters.suppressed_batches),
            discarded_events: load_u64(&self.counters.discarded_events),
            rescan_required: self.rescan_required.load(Ordering::Acquire),
        }
    }
}

#[derive(Clone)]
pub struct AssetWatchEventSender {
    sender: SyncSender<AssetWatchEventBatch>,
    shared: Arc<AssetWatchQueueShared>,
}

impl AssetWatchEventSender {
    pub fn try_send(&self, event: AssetWatchEvent) -> Result<(), AssetWatchQueueSendError> {
        self.try_send_batch(vec![event])
    }

    pub fn try_send_batch(
        &self,
        events: Vec<AssetWatchEvent>,
    ) -> Result<(), AssetWatchQueueSendError> {
        let event_count = events.len();
        if event_count == 0 {
            return Ok(());
        }
        if !self.shared.receiver_connected.load(Ordering::Acquire) {
            self.shared
                .record_failure(AssetWatchFailureKind::Disconnected, event_count, true);
            return Err(AssetWatchQueueSendError::Disconnected);
        }
        if self.shared.rescan_required.load(Ordering::Acquire) {
            saturating_increment(&self.shared.counters.suppressed_batches, 1);
            self.shared.record_discarded(event_count);
            return Err(AssetWatchQueueSendError::RescanRequired);
        }

        let mut next_sequence = match self.shared.next_sequence.try_lock() {
            Ok(sequence) => sequence,
            Err(TryLockError::WouldBlock) => {
                self.shared
                    .record_failure(AssetWatchFailureKind::Busy, event_count, true);
                return Err(AssetWatchQueueSendError::Busy);
            }
            Err(TryLockError::Poisoned(_)) => {
                self.shared.record_unavailable(event_count);
                return Err(AssetWatchQueueSendError::Unavailable);
            }
        };
        if !self.shared.receiver_connected.load(Ordering::Acquire) {
            self.shared
                .record_failure(AssetWatchFailureKind::Disconnected, event_count, true);
            return Err(AssetWatchQueueSendError::Disconnected);
        }
        if self.shared.rescan_required.load(Ordering::Acquire) {
            saturating_increment(&self.shared.counters.suppressed_batches, 1);
            self.shared.record_discarded(event_count);
            return Err(AssetWatchQueueSendError::RescanRequired);
        }
        let Some(following_sequence) = next_sequence.checked_add(1) else {
            self.shared.record_unavailable(event_count);
            return Err(AssetWatchQueueSendError::Unavailable);
        };

        let batch = AssetWatchEventBatch::new(*next_sequence, events);
        let requested_events = match AssetWatchQueueShared::reserve(
            &self.shared.retained_events,
            event_count,
            self.shared.limits.events().get(),
        ) {
            Ok(requested) => requested,
            Err(requested_events) => {
                let requested_bytes = self
                    .shared
                    .retained_bytes
                    .load(Ordering::Acquire)
                    .saturating_add(batch.retained_bytes);
                self.shared
                    .record_failure(AssetWatchFailureKind::Overflow, event_count, true);
                return Err(AssetWatchQueueSendError::Full {
                    requested_events,
                    maximum_events: self.shared.limits.events().get(),
                    requested_bytes,
                    maximum_bytes: self.shared.limits.retained_bytes().get(),
                });
            }
        };
        let requested_bytes = match AssetWatchQueueShared::reserve(
            &self.shared.retained_bytes,
            batch.retained_bytes,
            self.shared.limits.retained_bytes().get(),
        ) {
            Ok(requested) => requested,
            Err(requested_bytes) => {
                AssetWatchQueueShared::release(&self.shared.retained_events, event_count);
                self.shared
                    .record_failure(AssetWatchFailureKind::Overflow, event_count, true);
                return Err(AssetWatchQueueSendError::Full {
                    requested_events,
                    maximum_events: self.shared.limits.events().get(),
                    requested_bytes,
                    maximum_bytes: self.shared.limits.retained_bytes().get(),
                });
            }
        };

        match self.sender.try_send(batch) {
            Ok(()) => {}
            Err(TrySendError::Full(batch)) => {
                AssetWatchQueueShared::release(&self.shared.retained_events, event_count);
                AssetWatchQueueShared::release(&self.shared.retained_bytes, batch.retained_bytes);
                self.shared
                    .record_failure(AssetWatchFailureKind::Overflow, event_count, true);
                return Err(AssetWatchQueueSendError::Full {
                    requested_events,
                    maximum_events: self.shared.limits.events().get(),
                    requested_bytes,
                    maximum_bytes: self.shared.limits.retained_bytes().get(),
                });
            }
            Err(TrySendError::Disconnected(batch)) => {
                AssetWatchQueueShared::release(&self.shared.retained_events, event_count);
                AssetWatchQueueShared::release(&self.shared.retained_bytes, batch.retained_bytes);
                self.shared
                    .record_failure(AssetWatchFailureKind::Disconnected, event_count, true);
                return Err(AssetWatchQueueSendError::Disconnected);
            }
        }

        self.shared
            .high_water_events
            .fetch_max(requested_events, Ordering::AcqRel);
        self.shared
            .high_water_bytes
            .fetch_max(requested_bytes, Ordering::AcqRel);
        saturating_increment(&self.shared.counters.accepted_batches, 1);
        saturating_increment(
            &self.shared.counters.accepted_events,
            usize_to_u64(event_count),
        );
        self.shared
            .published_sequence
            .store(*next_sequence, Ordering::Release);
        *next_sequence = following_sequence;
        Ok(())
    }

    #[must_use]
    pub fn stats(&self) -> AssetWatchQueueStats {
        self.shared.stats()
    }

    pub(crate) fn record_callback_translation_failure(&self, discarded_events: usize) {
        self.shared.record_translation_failure(discarded_events);
    }

    pub(crate) fn record_backend_failure(&self) {
        self.shared.record_backend_failure();
    }

    #[cfg(test)]
    pub(crate) fn with_admission_held_for_tests<R>(&self, operation: impl FnOnce() -> R) -> R {
        let _guard = self
            .shared
            .next_sequence
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operation()
    }
}

impl Debug for AssetWatchEventSender {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatchEventSender")
            .field("stats", &self.stats())
            .finish()
    }
}

#[derive(Debug)]
pub struct AssetWatchQueueDrain {
    events: Vec<AssetWatchEvent>,
    captured_events: usize,
    rescan_required: bool,
}

impl AssetWatchQueueDrain {
    #[must_use]
    pub fn events(&self) -> &[AssetWatchEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<AssetWatchEvent> {
        self.events
    }

    #[must_use]
    pub const fn captured_events(&self) -> usize {
        self.captured_events
    }

    #[must_use]
    pub const fn rescan_required(&self) -> bool {
        self.rescan_required
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetWatchFailureCounters {
    values: [u64; AssetWatchFailureKind::COUNT],
}

impl Default for AssetWatchFailureCounters {
    fn default() -> Self {
        Self {
            values: [0; AssetWatchFailureKind::COUNT],
        }
    }
}

impl AssetWatchFailureCounters {
    fn from_stats(stats: AssetWatchQueueStats) -> Self {
        Self {
            values: std::array::from_fn(|index| stats.failures[index]),
        }
    }

    fn saturating_sub(self, previous: Self) -> Self {
        Self {
            values: std::array::from_fn(|index| {
                self.values[index].saturating_sub(previous.values[index])
            }),
        }
    }

    pub(crate) const fn get(self, kind: AssetWatchFailureKind) -> u64 {
        self.values[kind.index()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetWatchQueueObservation {
    pub(crate) stats: AssetWatchQueueStats,
    pub(crate) new_failures: AssetWatchFailureCounters,
    pub(crate) rescan_started: bool,
}

#[derive(Resource)]
pub(crate) struct AssetWatchQueueObserver {
    shared: Arc<AssetWatchQueueShared>,
    observed_failures: AssetWatchFailureCounters,
    observed_rescan: bool,
}

impl AssetWatchQueueObserver {
    pub(crate) fn record_translation_failure(&self, discarded_events: usize) {
        self.shared.record_translation_failure(discarded_events);
    }

    pub(crate) fn record_source_admission_failure(&self, discarded_events: usize) {
        self.shared
            .record_source_admission_failure(discarded_events);
    }

    pub(crate) fn observe(&mut self) -> AssetWatchQueueObservation {
        let stats = self.shared.stats();
        let failures = AssetWatchFailureCounters::from_stats(stats);
        let new_failures = failures.saturating_sub(self.observed_failures);
        let rescan_started = stats.rescan_required() && !self.observed_rescan;
        self.observed_failures = failures;
        self.observed_rescan = stats.rescan_required();
        AssetWatchQueueObservation {
            stats,
            new_failures,
            rescan_started,
        }
    }
}

#[derive(Resource)]
pub struct AssetWatchEventQueue {
    sender: SyncSender<AssetWatchEventBatch>,
    receiver: Mutex<Receiver<AssetWatchEventBatch>>,
    deferred: Option<AssetWatchEventBatch>,
    shared: Arc<AssetWatchQueueShared>,
}

impl Default for AssetWatchEventQueue {
    fn default() -> Self {
        Self::with_limits(AssetWatchQueueLimits::default())
    }
}

impl AssetWatchEventQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_limits(limits: AssetWatchQueueLimits) -> Self {
        let (sender, receiver) = sync_channel(limits.events().get());
        Self {
            sender,
            receiver: Mutex::new(receiver),
            deferred: None,
            shared: Arc::new(AssetWatchQueueShared::new(limits)),
        }
    }

    #[must_use]
    pub fn sender(&self) -> AssetWatchEventSender {
        AssetWatchEventSender {
            sender: self.sender.clone(),
            shared: Arc::clone(&self.shared),
        }
    }

    pub(crate) fn observer(&self) -> AssetWatchQueueObserver {
        AssetWatchQueueObserver {
            shared: Arc::clone(&self.shared),
            observed_failures: AssetWatchFailureCounters::default(),
            observed_rescan: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_receiver_held_for_tests<R>(&self, operation: impl FnOnce() -> R) -> R {
        let _guard = self
            .receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operation()
    }

    pub fn drain(&mut self) -> AssetWatchQueueDrain {
        let cutoff = self.shared.published_sequence.load(Ordering::Acquire);
        let receiver = self
            .receiver
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut batches = VecDeque::new();
        loop {
            let batch = if let Some(batch) = self.deferred.take() {
                batch
            } else {
                match receiver.try_recv() {
                    Ok(batch) => batch,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            };
            if batch.sequence > cutoff {
                self.deferred = Some(batch);
                break;
            }
            AssetWatchQueueShared::release(&self.shared.retained_events, batch.events.len());
            AssetWatchQueueShared::release(&self.shared.retained_bytes, batch.retained_bytes);
            batches.push_back(batch);
        }

        let captured_events = batches
            .iter()
            .map(|batch| batch.events.len())
            .fold(0, usize::saturating_add);
        let rescan_required = self.shared.rescan_required.load(Ordering::Acquire);
        if rescan_required {
            self.shared.record_discarded(captured_events);
            batches.clear();
        }
        let mut events = Vec::with_capacity(if rescan_required { 0 } else { captured_events });
        for batch in batches {
            events.extend(batch.events);
        }
        AssetWatchQueueDrain {
            events,
            captured_events,
            rescan_required,
        }
    }

    #[must_use]
    pub fn stats(&self) -> AssetWatchQueueStats {
        self.shared.stats()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.stats().retained_events()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for AssetWatchEventQueue {
    fn drop(&mut self) {
        self.shared
            .receiver_connected
            .store(false, Ordering::Release);
    }
}

impl Debug for AssetWatchEventQueue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatchEventQueue")
            .field("limits", &self.shared.limits)
            .field("stats", &self.stats())
            .finish()
    }
}

fn saturating_increment(counter: &AtomicU64, amount: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(amount))
    });
}

pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
