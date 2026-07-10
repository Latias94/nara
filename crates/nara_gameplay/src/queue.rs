use std::{collections::BTreeMap, num::NonZeroU64};

use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Resource;
use thiserror::Error;

use crate::{
    GameplayCommandDraft, GameplayCommandEnvelope, GameplayCommandKey, GameplayCommandSource,
    GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
};

pub const MAX_GAMEPLAY_COMMAND_RETAINED_COMMANDS: usize = 65_536;
pub const MAX_GAMEPLAY_COMMAND_RETAINED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_GAMEPLAY_COMMAND_BYTES: usize = 512 * 1024;
pub const MAX_GAMEPLAY_COMMAND_FUTURE_TICKS: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameplayCommandQueueSettings {
    retained_commands: ItemLimit,
    retained_bytes: ByteLimit,
    command_bytes: ByteLimit,
    payload_items: ItemLimit,
    payload_bytes: ByteLimit,
    future_ticks: NonZeroU64,
}

impl Default for GameplayCommandQueueSettings {
    fn default() -> Self {
        Self {
            retained_commands: item_limit(4_096),
            retained_bytes: byte_limit(4 * 1024 * 1024),
            command_bytes: byte_limit(64 * 1024),
            payload_items: item_limit(64),
            payload_bytes: byte_limit(32 * 1024),
            future_ticks: NonZeroU64::new(600).expect("default future tick limit is non-zero"),
        }
    }
}

impl GameplayCommandQueueSettings {
    pub fn new(
        retained_commands: ItemLimit,
        retained_bytes: ByteLimit,
        command_bytes: ByteLimit,
        payload_items: ItemLimit,
        payload_bytes: ByteLimit,
        future_ticks: NonZeroU64,
    ) -> Result<Self, GameplayCommandSettingsError> {
        let settings = Self {
            retained_commands,
            retained_bytes,
            command_bytes,
            payload_items,
            payload_bytes,
            future_ticks,
        };
        settings.validate()?;
        Ok(settings)
    }

    #[must_use]
    pub const fn retained_commands(self) -> ItemLimit {
        self.retained_commands
    }

    #[must_use]
    pub const fn retained_bytes(self) -> ByteLimit {
        self.retained_bytes
    }

    #[must_use]
    pub const fn command_bytes(self) -> ByteLimit {
        self.command_bytes
    }

    #[must_use]
    pub const fn payload_items(self) -> ItemLimit {
        self.payload_items
    }

    #[must_use]
    pub const fn payload_bytes(self) -> ByteLimit {
        self.payload_bytes
    }

    #[must_use]
    pub const fn future_ticks(self) -> NonZeroU64 {
        self.future_ticks
    }

    fn validate(self) -> Result<(), GameplayCommandSettingsError> {
        validate_maximum(
            GameplayCommandLimitKind::RetainedCommands,
            self.retained_commands.get(),
            MAX_GAMEPLAY_COMMAND_RETAINED_COMMANDS,
        )?;
        validate_maximum(
            GameplayCommandLimitKind::RetainedBytes,
            self.retained_bytes.get(),
            MAX_GAMEPLAY_COMMAND_RETAINED_BYTES,
        )?;
        validate_maximum(
            GameplayCommandLimitKind::CommandBytes,
            self.command_bytes.get(),
            MAX_GAMEPLAY_COMMAND_BYTES,
        )?;
        validate_maximum(
            GameplayCommandLimitKind::PayloadItems,
            self.payload_items.get(),
            crate::MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS,
        )?;
        validate_maximum(
            GameplayCommandLimitKind::PayloadBytes,
            self.payload_bytes.get(),
            crate::MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES,
        )?;
        if self.future_ticks.get() > MAX_GAMEPLAY_COMMAND_FUTURE_TICKS {
            return Err(GameplayCommandSettingsError::LimitTooLarge {
                kind: GameplayCommandLimitKind::FutureTicks,
                requested: usize::try_from(self.future_ticks.get()).unwrap_or(usize::MAX),
                maximum: usize::try_from(MAX_GAMEPLAY_COMMAND_FUTURE_TICKS).unwrap_or(usize::MAX),
            });
        }
        if self.command_bytes.get() > self.retained_bytes.get() {
            return Err(GameplayCommandSettingsError::CommandExceedsRetainedBytes);
        }
        if self.payload_bytes.get() > self.command_bytes.get() {
            return Err(GameplayCommandSettingsError::PayloadExceedsCommandBytes);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayCommandLimitKind {
    RetainedCommands,
    RetainedBytes,
    CommandBytes,
    PayloadItems,
    PayloadBytes,
    FutureTicks,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandSettingsError {
    #[error("gameplay command limit exceeds its domain maximum")]
    LimitTooLarge {
        kind: GameplayCommandLimitKind,
        requested: usize,
        maximum: usize,
    },
    #[error("per-command byte limit cannot exceed retained byte limit")]
    CommandExceedsRetainedBytes,
    #[error("payload byte limit cannot exceed per-command byte limit")]
    PayloadExceedsCommandBytes,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandRejection {
    #[error("gameplay command queue lifecycle has failed")]
    LifecycleFaulted,
    #[error("gameplay command logical size overflowed")]
    LogicalSizeOverflow,
    #[error("gameplay command payload exceeds its item limit")]
    PayloadItemLimit { requested: usize, maximum: usize },
    #[error("gameplay command payload exceeds its byte limit")]
    PayloadByteLimit { requested: usize, maximum: usize },
    #[error("gameplay command exceeds its byte limit")]
    CommandByteLimit { requested: usize, maximum: usize },
    #[error("gameplay command targets a closed tick")]
    Late { target: u64, closed_through: u64 },
    #[error("gameplay command exceeds the future tick horizon")]
    TooFarFuture {
        target: u64,
        closed_through: u64,
        maximum_distance: u64,
    },
    #[error("gameplay command key is already retained")]
    Duplicate,
    #[error("gameplay command queue reached its item limit")]
    RetainedItemLimit { requested: usize, maximum: usize },
    #[error("gameplay command queue reached its byte limit")]
    RetainedByteLimit { requested: usize, maximum: usize },
    #[error("the next authoritative gameplay command tick cannot be represented")]
    TickExhausted,
    #[error("the local gameplay command source sequence cannot be represented")]
    SourceSequenceExhausted,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandLifecycleError {
    #[error("gameplay command lifecycle is already poisoned")]
    Poisoned,
    #[error("fixed command admission received tick zero")]
    ZeroTick,
    #[error("fixed command admission did not receive the next tick")]
    UnexpectedTick { expected: u64, actual: u64 },
    #[error("a gameplay command batch is already active")]
    BatchAlreadyActive,
    #[error("the gameplay command batch does not match the active tick")]
    BatchTickMismatch,
    #[error("gameplay command retained accounting is inconsistent")]
    AccountingInvariant,
    #[error("the next gameplay command lifecycle tick cannot be represented")]
    TickExhausted,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GameplayCommandQueueStats {
    pub accepted: u64,
    pub admitted: u64,
    pub acknowledged: u64,
    pub acknowledged_ticks: u64,
    pub rejected: u64,
    pub rejected_lifecycle: u64,
    pub rejected_invalid: u64,
    pub rejected_late: u64,
    pub rejected_future: u64,
    pub rejected_duplicate: u64,
    pub rejected_item_limit: u64,
    pub rejected_byte_limit: u64,
    pub rejected_exhausted: u64,
    pub lifecycle_faults: u64,
    pub pending_commands: usize,
    pub active_commands: usize,
    pub quarantined_commands: usize,
    pub retained_commands: usize,
    pub pending_bytes: usize,
    pub active_bytes: usize,
    pub quarantined_bytes: usize,
    pub retained_bytes: usize,
    pub closed_through_tick: u64,
    pub acknowledged_through_tick: u64,
    pub furthest_pending_tick: Option<u64>,
}

#[derive(Debug, Resource)]
pub struct GameplayCommandBatch {
    active_tick: Option<GameplayCommandTick>,
    commands: Vec<GameplayCommandEnvelope>,
    logical_bytes: usize,
}

impl GameplayCommandBatch {
    pub(crate) const fn new() -> Self {
        Self {
            active_tick: None,
            commands: Vec::new(),
            logical_bytes: 0,
        }
    }

    #[must_use]
    pub const fn active_tick(&self) -> Option<GameplayCommandTick> {
        self.active_tick
    }

    #[must_use]
    pub fn commands(&self) -> &[GameplayCommandEnvelope] {
        &self.commands
    }

    pub fn iter(&self) -> impl Iterator<Item = &GameplayCommandEnvelope> {
        self.commands.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> usize {
        self.logical_bytes
    }

    fn activate(
        &mut self,
        tick: GameplayCommandTick,
        commands: Vec<GameplayCommandEnvelope>,
        logical_bytes: usize,
    ) {
        self.active_tick = Some(tick);
        self.commands = commands;
        self.logical_bytes = logical_bytes;
    }

    fn retire(&mut self) {
        self.active_tick = None;
        self.commands.clear();
        self.logical_bytes = 0;
    }

    fn quarantine(&mut self) -> (Vec<GameplayCommandEnvelope>, usize) {
        self.active_tick = None;
        let commands = std::mem::take(&mut self.commands);
        let logical_bytes = std::mem::replace(&mut self.logical_bytes, 0);
        (commands, logical_bytes)
    }
}

#[derive(Debug)]
struct QueuedCommand {
    command: GameplayCommandDraft,
    logical_bytes: usize,
}

type SourceSequenceKey = (GameplayCommandSource, GameplayCommandSourceSequence);

#[derive(Debug, Default)]
struct GameplayCommandCounters {
    accepted: u64,
    admitted: u64,
    acknowledged: u64,
    acknowledged_ticks: u64,
    rejected: u64,
    rejected_lifecycle: u64,
    rejected_invalid: u64,
    rejected_late: u64,
    rejected_future: u64,
    rejected_duplicate: u64,
    rejected_item_limit: u64,
    rejected_byte_limit: u64,
    rejected_exhausted: u64,
    lifecycle_faults: u64,
}

/// Bounded authoritative gameplay intent waiting for fixed-tick admission.
///
/// Producers submit local semantic actions in `PreUpdate` or explicit replay/AI/external commands
/// before `GameplayCommandSet::Admit`. The queue retains accepted intent across zero-tick frames.
/// `Admit` moves one tick into `GameplayCommandBatch`; `Acknowledge` retires it after replay capture.
/// A lifecycle fault terminally poisons ingress, hides the public batch, and moves any admitted
/// envelopes into a queue-owned quarantine for inspection. Rejections and numeric statistics are
/// observable without logging. U31 owns the optional bridge into runtime diagnostics and pressure
/// snapshots.
#[derive(Debug, Resource)]
pub struct GameplayCommandQueue {
    settings: GameplayCommandQueueSettings,
    pending: BTreeMap<GameplayCommandTick, BTreeMap<SourceSequenceKey, QueuedCommand>>,
    quarantined: Vec<GameplayCommandEnvelope>,
    quarantined_bytes: usize,
    closed_through_tick: u64,
    acknowledged_through_tick: u64,
    pending_commands: usize,
    pending_bytes: usize,
    retained_commands: usize,
    retained_bytes: usize,
    active_commands: usize,
    active_bytes: usize,
    last_local_sequence: u64,
    counters: GameplayCommandCounters,
    last_lifecycle_error: Option<GameplayCommandLifecycleError>,
}

impl Default for GameplayCommandQueue {
    fn default() -> Self {
        Self::new(GameplayCommandQueueSettings::default())
    }
}

impl GameplayCommandQueue {
    #[must_use]
    pub fn new(settings: GameplayCommandQueueSettings) -> Self {
        Self {
            settings,
            pending: BTreeMap::new(),
            quarantined: Vec::new(),
            quarantined_bytes: 0,
            closed_through_tick: 0,
            acknowledged_through_tick: 0,
            pending_commands: 0,
            pending_bytes: 0,
            retained_commands: 0,
            retained_bytes: 0,
            active_commands: 0,
            active_bytes: 0,
            last_local_sequence: 0,
            counters: GameplayCommandCounters::default(),
            last_lifecycle_error: None,
        }
    }

    #[must_use]
    pub const fn settings(&self) -> GameplayCommandQueueSettings {
        self.settings
    }

    pub fn submit(
        &mut self,
        submission: GameplayCommandSubmission,
    ) -> Result<GameplayCommandKey, GameplayCommandRejection> {
        let (key, command) = submission.into_parts();
        self.submit_keyed(key, command)
    }

    fn submit_keyed(
        &mut self,
        key: GameplayCommandKey,
        command: GameplayCommandDraft,
    ) -> Result<GameplayCommandKey, GameplayCommandRejection> {
        if self.last_lifecycle_error.is_some() {
            return self.reject(GameplayCommandRejection::LifecycleFaulted);
        }
        let payload_items = command.payload().len();
        if payload_items > self.settings.payload_items.get() {
            return self.reject(GameplayCommandRejection::PayloadItemLimit {
                requested: payload_items,
                maximum: self.settings.payload_items.get(),
            });
        }
        let payload_bytes = command.payload().logical_bytes();
        if payload_bytes > self.settings.payload_bytes.get() {
            return self.reject(GameplayCommandRejection::PayloadByteLimit {
                requested: payload_bytes,
                maximum: self.settings.payload_bytes.get(),
            });
        }
        let Some(logical_bytes) = key.logical_bytes().and_then(|bytes| {
            command
                .logical_bytes()
                .and_then(|command| bytes.checked_add(command))
        }) else {
            return self.reject(GameplayCommandRejection::LogicalSizeOverflow);
        };
        if logical_bytes > self.settings.command_bytes.get() {
            return self.reject(GameplayCommandRejection::CommandByteLimit {
                requested: logical_bytes,
                maximum: self.settings.command_bytes.get(),
            });
        }

        let target_tick = key.tick().get();
        if target_tick <= self.closed_through_tick {
            return self.reject(GameplayCommandRejection::Late {
                target: target_tick,
                closed_through: self.closed_through_tick,
            });
        }
        let Some(future_distance) = target_tick.checked_sub(self.closed_through_tick) else {
            return self.reject(GameplayCommandRejection::Late {
                target: target_tick,
                closed_through: self.closed_through_tick,
            });
        };
        if future_distance > self.settings.future_ticks.get() {
            return self.reject(GameplayCommandRejection::TooFarFuture {
                target: target_tick,
                closed_through: self.closed_through_tick,
                maximum_distance: self.settings.future_ticks.get(),
            });
        }

        let source_sequence = (key.source().clone(), key.source_sequence());
        if self
            .pending
            .get(&key.tick())
            .is_some_and(|commands| commands.contains_key(&source_sequence))
        {
            return self.reject(GameplayCommandRejection::Duplicate);
        }

        let Some(next_commands) = self.retained_commands.checked_add(1) else {
            return self.reject(GameplayCommandRejection::RetainedItemLimit {
                requested: usize::MAX,
                maximum: self.settings.retained_commands.get(),
            });
        };
        if next_commands > self.settings.retained_commands.get() {
            return self.reject(GameplayCommandRejection::RetainedItemLimit {
                requested: next_commands,
                maximum: self.settings.retained_commands.get(),
            });
        }
        let Some(next_bytes) = self.retained_bytes.checked_add(logical_bytes) else {
            return self.reject(GameplayCommandRejection::RetainedByteLimit {
                requested: usize::MAX,
                maximum: self.settings.retained_bytes.get(),
            });
        };
        if next_bytes > self.settings.retained_bytes.get() {
            return self.reject(GameplayCommandRejection::RetainedByteLimit {
                requested: next_bytes,
                maximum: self.settings.retained_bytes.get(),
            });
        }

        let Some(next_pending_commands) = self.pending_commands.checked_add(1) else {
            return self.reject(GameplayCommandRejection::RetainedItemLimit {
                requested: usize::MAX,
                maximum: self.settings.retained_commands.get(),
            });
        };
        let Some(next_pending_bytes) = self.pending_bytes.checked_add(logical_bytes) else {
            return self.reject(GameplayCommandRejection::RetainedByteLimit {
                requested: usize::MAX,
                maximum: self.settings.retained_bytes.get(),
            });
        };

        let tick = key.tick();
        let source_sequence = key.clone().into_source_sequence();
        self.pending.entry(tick).or_default().insert(
            source_sequence,
            QueuedCommand {
                command,
                logical_bytes,
            },
        );
        self.pending_commands = next_pending_commands;
        self.pending_bytes = next_pending_bytes;
        self.retained_commands = next_commands;
        self.retained_bytes = next_bytes;
        self.counters.accepted = self.counters.accepted.saturating_add(1);
        Ok(key)
    }

    #[must_use]
    pub fn stats(&self) -> GameplayCommandQueueStats {
        GameplayCommandQueueStats {
            accepted: self.counters.accepted,
            admitted: self.counters.admitted,
            acknowledged: self.counters.acknowledged,
            acknowledged_ticks: self.counters.acknowledged_ticks,
            rejected: self.counters.rejected,
            rejected_lifecycle: self.counters.rejected_lifecycle,
            rejected_invalid: self.counters.rejected_invalid,
            rejected_late: self.counters.rejected_late,
            rejected_future: self.counters.rejected_future,
            rejected_duplicate: self.counters.rejected_duplicate,
            rejected_item_limit: self.counters.rejected_item_limit,
            rejected_byte_limit: self.counters.rejected_byte_limit,
            rejected_exhausted: self.counters.rejected_exhausted,
            lifecycle_faults: self.counters.lifecycle_faults,
            pending_commands: self.pending_commands,
            active_commands: self.active_commands,
            quarantined_commands: self.quarantined.len(),
            retained_commands: self.retained_commands,
            pending_bytes: self.pending_bytes,
            active_bytes: self.active_bytes,
            quarantined_bytes: self.quarantined_bytes,
            retained_bytes: self.retained_bytes,
            closed_through_tick: self.closed_through_tick,
            acknowledged_through_tick: self.acknowledged_through_tick,
            furthest_pending_tick: self.pending.last_key_value().map(|(tick, _)| tick.get()),
        }
    }

    #[must_use]
    pub const fn last_lifecycle_error(&self) -> Option<&GameplayCommandLifecycleError> {
        self.last_lifecycle_error.as_ref()
    }

    #[must_use]
    pub fn quarantined_commands(&self) -> &[GameplayCommandEnvelope] {
        &self.quarantined
    }

    #[must_use]
    pub const fn is_idle(&self) -> bool {
        self.last_lifecycle_error.is_none()
            && self.retained_commands == 0
            && self.closed_through_tick == self.acknowledged_through_tick
    }

    pub(crate) fn submit_local_for_next_tick(
        &mut self,
        command: GameplayCommandDraft,
    ) -> Result<GameplayCommandKey, GameplayCommandRejection> {
        if self.last_lifecycle_error.is_some() {
            return self.reject(GameplayCommandRejection::LifecycleFaulted);
        }
        let Some(tick) = self
            .closed_through_tick
            .checked_add(1)
            .and_then(GameplayCommandTick::new)
        else {
            return self.reject(GameplayCommandRejection::TickExhausted);
        };
        let Some(sequence) = self
            .last_local_sequence
            .checked_add(1)
            .and_then(GameplayCommandSourceSequence::new)
        else {
            return self.reject(GameplayCommandRejection::SourceSequenceExhausted);
        };
        let key = GameplayCommandKey::new(tick, GameplayCommandSource::LocalAction, sequence);
        let key = self.submit_keyed(key, command)?;
        self.last_local_sequence = sequence.get();
        Ok(key)
    }

    pub(crate) fn admit_tick(
        &mut self,
        tick: GameplayCommandTick,
        batch: &mut GameplayCommandBatch,
    ) -> Result<(), GameplayCommandLifecycleError> {
        if self.last_lifecycle_error.is_some() {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::Poisoned);
        }
        if batch.active_tick.is_some() || self.closed_through_tick != self.acknowledged_through_tick
        {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::BatchAlreadyActive);
        }
        let Some(expected) = self.acknowledged_through_tick.checked_add(1) else {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::TickExhausted);
        };
        if tick.get() != expected {
            return self.invalidate_batch(
                batch,
                GameplayCommandLifecycleError::UnexpectedTick {
                    expected,
                    actual: tick.get(),
                },
            );
        }

        let Some((active_commands, active_bytes)) =
            self.pending.get(&tick).map_or(Some((0, 0)), |bucket| {
                bucket
                    .values()
                    .try_fold((0_usize, 0_usize), |(items, bytes), command| {
                        Some((
                            items.checked_add(1)?,
                            bytes.checked_add(command.logical_bytes)?,
                        ))
                    })
            })
        else {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        };
        let Some(next_pending_commands) = self.pending_commands.checked_sub(active_commands) else {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        };
        let Some(next_pending_bytes) = self.pending_bytes.checked_sub(active_bytes) else {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        };
        if active_commands > self.retained_commands || active_bytes > self.retained_bytes {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        }

        let commands = self
            .pending
            .remove(&tick)
            .unwrap_or_default()
            .into_iter()
            .map(|((source, source_sequence), queued)| {
                GameplayCommandEnvelope::admitted(
                    GameplayCommandKey::new(tick, source, source_sequence),
                    queued.command,
                    queued.logical_bytes,
                )
            })
            .collect();
        self.pending_commands = next_pending_commands;
        self.pending_bytes = next_pending_bytes;
        self.closed_through_tick = tick.get();
        self.active_commands = active_commands;
        self.active_bytes = active_bytes;
        self.counters.admitted = self
            .counters
            .admitted
            .saturating_add(u64::try_from(active_commands).unwrap_or(u64::MAX));
        batch.activate(tick, commands, active_bytes);
        Ok(())
    }

    pub(crate) fn admit_fixed_tick(
        &mut self,
        tick: u64,
        batch: &mut GameplayCommandBatch,
    ) -> Result<(), GameplayCommandLifecycleError> {
        let Some(tick) = GameplayCommandTick::new(tick) else {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::ZeroTick);
        };
        self.admit_tick(tick, batch)
    }

    pub(crate) fn acknowledge_tick(
        &mut self,
        tick: GameplayCommandTick,
        batch: &mut GameplayCommandBatch,
    ) -> Result<(), GameplayCommandLifecycleError> {
        if self.last_lifecycle_error.is_some() {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::Poisoned);
        }
        if batch.active_tick != Some(tick) || self.closed_through_tick != tick.get() {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::BatchTickMismatch);
        }
        if batch.len() != self.active_commands || batch.logical_bytes() != self.active_bytes {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        }
        let Some(next_commands) = self.retained_commands.checked_sub(self.active_commands) else {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        };
        let Some(next_bytes) = self.retained_bytes.checked_sub(self.active_bytes) else {
            return self
                .invalidate_batch(batch, GameplayCommandLifecycleError::AccountingInvariant);
        };

        let acknowledged = self.active_commands;
        self.retained_commands = next_commands;
        self.retained_bytes = next_bytes;
        self.active_commands = 0;
        self.active_bytes = 0;
        self.acknowledged_through_tick = tick.get();
        self.counters.acknowledged = self
            .counters
            .acknowledged
            .saturating_add(u64::try_from(acknowledged).unwrap_or(u64::MAX));
        self.counters.acknowledged_ticks = self.counters.acknowledged_ticks.saturating_add(1);
        batch.retire();
        Ok(())
    }

    pub(crate) fn acknowledge_fixed_tick(
        &mut self,
        tick: u64,
        batch: &mut GameplayCommandBatch,
    ) -> Result<(), GameplayCommandLifecycleError> {
        let Some(tick) = GameplayCommandTick::new(tick) else {
            return self.invalidate_batch(batch, GameplayCommandLifecycleError::ZeroTick);
        };
        self.acknowledge_tick(tick, batch)
    }

    fn reject<T>(
        &mut self,
        rejection: GameplayCommandRejection,
    ) -> Result<T, GameplayCommandRejection> {
        self.counters.rejected = self.counters.rejected.saturating_add(1);
        match rejection {
            GameplayCommandRejection::LifecycleFaulted => {
                self.counters.rejected_lifecycle =
                    self.counters.rejected_lifecycle.saturating_add(1);
            }
            GameplayCommandRejection::LogicalSizeOverflow
            | GameplayCommandRejection::PayloadItemLimit { .. }
            | GameplayCommandRejection::PayloadByteLimit { .. }
            | GameplayCommandRejection::CommandByteLimit { .. } => {
                self.counters.rejected_invalid = self.counters.rejected_invalid.saturating_add(1);
            }
            GameplayCommandRejection::Late { .. } => {
                self.counters.rejected_late = self.counters.rejected_late.saturating_add(1);
            }
            GameplayCommandRejection::TooFarFuture { .. } => {
                self.counters.rejected_future = self.counters.rejected_future.saturating_add(1);
            }
            GameplayCommandRejection::Duplicate => {
                self.counters.rejected_duplicate =
                    self.counters.rejected_duplicate.saturating_add(1);
            }
            GameplayCommandRejection::RetainedItemLimit { .. } => {
                self.counters.rejected_item_limit =
                    self.counters.rejected_item_limit.saturating_add(1);
            }
            GameplayCommandRejection::RetainedByteLimit { .. } => {
                self.counters.rejected_byte_limit =
                    self.counters.rejected_byte_limit.saturating_add(1);
            }
            GameplayCommandRejection::TickExhausted
            | GameplayCommandRejection::SourceSequenceExhausted => {
                self.counters.rejected_exhausted =
                    self.counters.rejected_exhausted.saturating_add(1);
            }
        }
        Err(rejection)
    }

    fn lifecycle_fault<T>(
        &mut self,
        error: GameplayCommandLifecycleError,
    ) -> Result<T, GameplayCommandLifecycleError> {
        self.counters.lifecycle_faults = self.counters.lifecycle_faults.saturating_add(1);
        if self.last_lifecycle_error.is_none() {
            self.last_lifecycle_error = Some(error.clone());
        }
        Err(error)
    }

    fn invalidate_batch<T>(
        &mut self,
        batch: &mut GameplayCommandBatch,
        mut error: GameplayCommandLifecycleError,
    ) -> Result<T, GameplayCommandLifecycleError> {
        let (quarantined, batch_bytes) = batch.quarantine();
        let next_quarantined_bytes = self.quarantined_bytes.checked_add(batch_bytes);
        if let Some(next_quarantined_bytes) = next_quarantined_bytes {
            if self.quarantined.is_empty() {
                self.quarantined = quarantined;
            } else {
                self.quarantined.extend(quarantined);
            }
            self.quarantined_bytes = next_quarantined_bytes;
            self.active_commands = 0;
            self.active_bytes = 0;

            match (
                self.pending_commands.checked_add(self.quarantined.len()),
                self.pending_bytes.checked_add(self.quarantined_bytes),
            ) {
                (Some(retained_commands), Some(retained_bytes)) => {
                    self.retained_commands = retained_commands;
                    self.retained_bytes = retained_bytes;
                }
                _ => {
                    self.discard_owned_commands_after_accounting_overflow();
                    error = GameplayCommandLifecycleError::AccountingInvariant;
                }
            }
        } else {
            self.discard_owned_commands_after_accounting_overflow();
            error = GameplayCommandLifecycleError::AccountingInvariant;
        }

        let error = if self.last_lifecycle_error.is_some() {
            GameplayCommandLifecycleError::Poisoned
        } else {
            error
        };
        self.lifecycle_fault(error)
    }

    fn discard_owned_commands_after_accounting_overflow(&mut self) {
        self.pending.clear();
        self.quarantined.clear();
        self.pending_commands = 0;
        self.pending_bytes = 0;
        self.quarantined_bytes = 0;
        self.active_commands = 0;
        self.active_bytes = 0;
        self.retained_commands = 0;
        self.retained_bytes = 0;
    }

    #[cfg(test)]
    pub(crate) fn set_watermarks_for_test(&mut self, tick: u64) {
        self.closed_through_tick = tick;
        self.acknowledged_through_tick = tick;
    }

    #[cfg(test)]
    pub(crate) fn set_local_sequence_for_test(&mut self, sequence: u64) {
        self.last_local_sequence = sequence;
    }
}

fn validate_maximum(
    kind: GameplayCommandLimitKind,
    requested: usize,
    maximum: usize,
) -> Result<(), GameplayCommandSettingsError> {
    if requested > maximum {
        return Err(GameplayCommandSettingsError::LimitTooLarge {
            kind,
            requested,
            maximum,
        });
    }
    Ok(())
}

fn item_limit(value: usize) -> ItemLimit {
    ItemLimit::new(value).expect("gameplay command item defaults are non-zero")
}

fn byte_limit(value: usize) -> ByteLimit {
    ByteLimit::new(value).expect("gameplay command byte defaults are non-zero")
}
