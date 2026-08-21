use std::{borrow::Cow, collections::VecDeque, error::Error, fmt, num::NonZeroUsize};

pub(super) use super::ProcessOutputStream as ProcessLogStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessLogLimits {
    record_limit: NonZeroUsize,
    byte_limit: NonZeroUsize,
}

impl ProcessLogLimits {
    pub(super) const fn new(record_limit: NonZeroUsize, byte_limit: NonZeroUsize) -> Self {
        Self {
            record_limit,
            byte_limit,
        }
    }

    pub(super) const fn record_limit(self) -> usize {
        self.record_limit.get()
    }

    pub(super) const fn byte_limit(self) -> usize {
        self.byte_limit.get()
    }

    pub(super) fn try_from_counts(
        record_limit: usize,
        byte_limit: usize,
    ) -> Result<Self, ProcessLogLimitsError> {
        let record_limit =
            NonZeroUsize::new(record_limit).ok_or(ProcessLogLimitsError::ZeroRecordLimit)?;
        let byte_limit =
            NonZeroUsize::new(byte_limit).ok_or(ProcessLogLimitsError::ZeroByteLimit)?;
        Ok(Self::new(record_limit, byte_limit))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessLogLimitsError {
    ZeroRecordLimit,
    ZeroByteLimit,
}

impl fmt::Display for ProcessLogLimitsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroRecordLimit => "the process log record limit must be non-zero",
            Self::ZeroByteLimit => "the process log byte limit must be non-zero",
        })
    }
}

impl Error for ProcessLogLimitsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ProcessLogSequence(u64);

impl ProcessLogSequence {
    pub(super) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProcessLogEntry {
    sequence: ProcessLogSequence,
    stream: ProcessLogStream,
    bytes: Box<[u8]>,
    truncated_prefix_bytes: u64,
}

impl ProcessLogEntry {
    pub(super) const fn sequence(&self) -> ProcessLogSequence {
        self.sequence
    }

    pub(super) const fn stream(&self) -> ProcessLogStream {
        self.stream
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) const fn truncated_prefix_bytes(&self) -> u64 {
        self.truncated_prefix_bytes
    }

    pub(super) fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ProcessLogStats {
    observed_records: u64,
    observed_bytes: u64,
    retained_records: usize,
    retained_bytes: usize,
    dropped_records: u64,
    dropped_bytes: u64,
    truncated_records: u64,
}

impl ProcessLogStats {
    pub(super) const fn observed_records(self) -> u64 {
        self.observed_records
    }

    pub(super) const fn observed_bytes(self) -> u64 {
        self.observed_bytes
    }

    pub(super) const fn retained_records(self) -> usize {
        self.retained_records
    }

    pub(super) const fn retained_bytes(self) -> usize {
        self.retained_bytes
    }

    pub(super) const fn dropped_records(self) -> u64 {
        self.dropped_records
    }

    pub(super) const fn dropped_bytes(self) -> u64 {
        self.dropped_bytes
    }

    pub(super) const fn truncated_records(self) -> u64 {
        self.truncated_records
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessLogAppendOutcome {
    sequence: ProcessLogSequence,
    retained_bytes: usize,
    truncated_prefix_bytes: u64,
    evicted_records: u64,
    evicted_bytes: u64,
}

impl ProcessLogAppendOutcome {
    pub(super) const fn sequence(self) -> ProcessLogSequence {
        self.sequence
    }

    pub(super) const fn retained_bytes(self) -> usize {
        self.retained_bytes
    }

    pub(super) const fn truncated_prefix_bytes(self) -> u64 {
        self.truncated_prefix_bytes
    }

    pub(super) const fn evicted_records(self) -> u64 {
        self.evicted_records
    }

    pub(super) const fn evicted_bytes(self) -> u64 {
        self.evicted_bytes
    }

    pub(super) const fn truncated(self) -> bool {
        self.truncated_prefix_bytes > 0
    }

    pub(super) const fn overflowed(self) -> bool {
        self.truncated() || self.evicted_records > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessLogPurge {
    records: usize,
    bytes: usize,
}

impl ProcessLogPurge {
    pub(super) const fn records(self) -> usize {
        self.records
    }

    pub(super) const fn bytes(self) -> usize {
        self.bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessLogSequenceExhausted;

impl fmt::Display for ProcessLogSequenceExhausted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the process log observation sequence is exhausted")
    }
}

impl Error for ProcessLogSequenceExhausted {}

#[derive(Debug)]
pub(super) struct ProcessLogRing {
    limits: ProcessLogLimits,
    records: VecDeque<ProcessLogEntry>,
    retained_bytes: usize,
    next_sequence: Option<u64>,
    observed_records: u64,
    observed_bytes: u64,
    dropped_records: u64,
    dropped_bytes: u64,
    truncated_records: u64,
}

impl ProcessLogRing {
    pub(super) fn new(limits: ProcessLogLimits) -> Self {
        Self {
            limits,
            records: VecDeque::new(),
            retained_bytes: 0,
            next_sequence: Some(1),
            observed_records: 0,
            observed_bytes: 0,
            dropped_records: 0,
            dropped_bytes: 0,
            truncated_records: 0,
        }
    }

    pub(super) fn append(
        &mut self,
        stream: ProcessLogStream,
        bytes: impl AsRef<[u8]>,
    ) -> Result<ProcessLogAppendOutcome, ProcessLogSequenceExhausted> {
        let sequence = self.take_sequence()?;
        let bytes = bytes.as_ref();
        let observed_bytes = bytes.len();
        let truncated_prefix_bytes = observed_bytes.saturating_sub(self.limits.byte_limit());
        if truncated_prefix_bytes > 0 {
            self.truncated_records = self.truncated_records.saturating_add(1);
            self.dropped_bytes = self
                .dropped_bytes
                .saturating_add(usize_to_u64(truncated_prefix_bytes));
        }
        let bytes = Box::<[u8]>::from(&bytes[truncated_prefix_bytes..]);
        let retained_bytes = bytes.len();

        let mut evicted_records = 0_u64;
        let mut evicted_bytes = 0_u64;
        while self.records.len() >= self.limits.record_limit()
            || self.retained_bytes.saturating_add(retained_bytes) > self.limits.byte_limit()
        {
            let Some(mut evicted) = self.records.pop_front() else {
                break;
            };
            let bytes = evicted.bytes.len();
            evicted.bytes.fill(0);
            self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
            evicted_records = evicted_records.saturating_add(1);
            evicted_bytes = evicted_bytes.saturating_add(usize_to_u64(bytes));
        }

        self.observed_records = self.observed_records.saturating_add(1);
        self.observed_bytes = self
            .observed_bytes
            .saturating_add(usize_to_u64(observed_bytes));
        self.dropped_records = self.dropped_records.saturating_add(evicted_records);
        self.dropped_bytes = self.dropped_bytes.saturating_add(evicted_bytes);
        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        self.records.push_back(ProcessLogEntry {
            sequence,
            stream,
            bytes,
            truncated_prefix_bytes: usize_to_u64(truncated_prefix_bytes),
        });

        Ok(ProcessLogAppendOutcome {
            sequence,
            retained_bytes,
            truncated_prefix_bytes: usize_to_u64(truncated_prefix_bytes),
            evicted_records,
            evicted_bytes,
        })
    }

    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = &ProcessLogEntry> {
        self.records.iter()
    }

    pub(super) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub(super) fn len(&self) -> usize {
        self.records.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(super) fn stats(&self) -> ProcessLogStats {
        ProcessLogStats {
            observed_records: self.observed_records,
            observed_bytes: self.observed_bytes,
            retained_records: self.records.len(),
            retained_bytes: self.retained_bytes,
            dropped_records: self.dropped_records,
            dropped_bytes: self.dropped_bytes,
            truncated_records: self.truncated_records,
        }
    }

    /// Removes all retained raw output while preserving lifetime counters and sequence ordering.
    ///
    /// Payloads are overwritten before release as lifecycle hygiene. This is not a secure-memory
    /// erasure guarantee.
    pub(super) fn purge(&mut self) -> ProcessLogPurge {
        let purge = ProcessLogPurge {
            records: self.records.len(),
            bytes: self.retained_bytes,
        };
        for record in &mut self.records {
            record.bytes.fill(0);
        }
        self.records.clear();
        self.records.shrink_to_fit();
        self.retained_bytes = 0;
        purge
    }

    fn take_sequence(&mut self) -> Result<ProcessLogSequence, ProcessLogSequenceExhausted> {
        let sequence = self.next_sequence.ok_or(ProcessLogSequenceExhausted)?;
        self.next_sequence = sequence.checked_add(1);
        Ok(ProcessLogSequence(sequence))
    }
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_evicts_the_oldest_records_and_accounts_for_loss() {
        let mut ring = ProcessLogRing::new(limits(2, 64));

        append(&mut ring, ProcessLogStream::Stdout, b"first");
        append(&mut ring, ProcessLogStream::Stderr, b"second");
        let third_append = append(&mut ring, ProcessLogStream::Stdout, b"third");

        let records = ring.iter().collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].text(), "second");
        assert_eq!(records[1].text(), "third");
        assert_eq!(third_append.evicted_records(), 1);
        assert_eq!(third_append.evicted_bytes(), 5);
        assert!(third_append.overflowed());
        assert_eq!(
            ring.stats(),
            ProcessLogStats {
                observed_records: 3,
                observed_bytes: 16,
                retained_records: 2,
                retained_bytes: 11,
                dropped_records: 1,
                dropped_bytes: 5,
                truncated_records: 0,
            }
        );

        let mut byte_limited = ProcessLogRing::new(limits(8, 6));
        append(&mut byte_limited, ProcessLogStream::Stdout, b"abc");
        append(&mut byte_limited, ProcessLogStream::Stderr, b"def");
        let byte_append = append(&mut byte_limited, ProcessLogStream::Stdout, b"gh");
        assert_eq!(byte_append.evicted_records(), 1);
        assert_eq!(byte_append.evicted_bytes(), 3);
        assert_eq!(
            byte_limited
                .iter()
                .map(|entry| entry.text().into_owned())
                .collect::<Vec<_>>(),
            ["def".to_owned(), "gh".to_owned()]
        );
    }

    #[test]
    fn oversized_record_keeps_the_newest_bytes_and_projects_invalid_utf8_lossily() {
        let mut ring = ProcessLogRing::new(limits(8, 4));
        let append = append(
            &mut ring,
            ProcessLogStream::Stderr,
            &[b'a', b'b', 0x9f, 0x92, 0xa9, b'z'],
        );

        let record = ring.iter().next().expect("one record is retained");
        assert_eq!(record.bytes(), &[0x9f, 0x92, 0xa9, b'z']);
        assert_eq!(record.truncated_prefix_bytes(), 2);
        assert!(record.text().contains('\u{fffd}'));
        assert!(record.text().ends_with('z'));
        assert_eq!(append.retained_bytes(), 4);
        assert_eq!(append.truncated_prefix_bytes(), 2);
        assert!(append.truncated());
        assert_eq!(ring.stats().truncated_records(), 1);
        assert_eq!(ring.stats().dropped_records(), 0);
        assert_eq!(ring.stats().dropped_bytes(), 2);
    }

    #[test]
    fn interleaved_streams_receive_one_monotonic_observation_order() {
        let mut ring = ProcessLogRing::new(limits(8, 64));
        append(&mut ring, ProcessLogStream::Stdout, b"one");
        append(&mut ring, ProcessLogStream::Stderr, b"two");
        append(&mut ring, ProcessLogStream::Stdout, b"three");

        let facts = ring
            .iter()
            .map(|record| {
                (
                    record.sequence().get(),
                    record.stream(),
                    record.text().into_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            facts,
            vec![
                (1, ProcessLogStream::Stdout, "one".to_owned()),
                (2, ProcessLogStream::Stderr, "two".to_owned()),
                (3, ProcessLogStream::Stdout, "three".to_owned()),
            ]
        );
    }

    #[test]
    fn purge_removes_raw_output_without_resetting_sequence_or_loss_history() {
        let mut ring = ProcessLogRing::new(limits(1, 4));
        append(&mut ring, ProcessLogStream::Stdout, b"first");
        let before = ring.stats();

        let purge = ring.purge();

        assert_eq!(purge.records(), 1);
        assert_eq!(purge.bytes(), 4);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.retained_bytes(), 0);
        assert_eq!(ring.stats().dropped_bytes(), before.dropped_bytes());
        assert_eq!(
            append(&mut ring, ProcessLogStream::Stderr, b"next")
                .sequence()
                .get(),
            2
        );
        assert_eq!(ring.stats().observed_records(), 2);
        assert_eq!(ring.stats().observed_bytes(), 9);
        assert_eq!(ring.stats().retained_records(), 1);
        assert_eq!(ring.stats().retained_bytes(), 4);
    }

    #[test]
    fn limits_reject_zero_record_or_byte_capacity() {
        assert_eq!(
            ProcessLogLimits::try_from_counts(0, 1),
            Err(ProcessLogLimitsError::ZeroRecordLimit)
        );
        assert_eq!(
            ProcessLogLimits::try_from_counts(1, 0),
            Err(ProcessLogLimitsError::ZeroByteLimit)
        );
    }

    fn limits(records: usize, bytes: usize) -> ProcessLogLimits {
        ProcessLogLimits::try_from_counts(records, bytes).expect("the test limits are non-zero")
    }

    fn append(
        ring: &mut ProcessLogRing,
        stream: ProcessLogStream,
        bytes: impl AsRef<[u8]>,
    ) -> ProcessLogAppendOutcome {
        ring.append(stream, bytes)
            .expect("the process log observation sequence remains available")
    }
}
