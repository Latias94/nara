use std::{
    collections::VecDeque,
    io,
    num::NonZeroUsize,
    sync::{Mutex, MutexGuard},
};

use super::{PlatformOutput, SupervisedProcessStream};
use crate::supervised_play::wake::SupervisedPlayWake;

const PROTOCOL_STDOUT_CHUNK_BYTES: usize = 16 * 1024;

/// Private, thread-safe queue between platform readers and the supervised-process facade.
///
/// Logs retain the newest bounded output and report discarded bytes. Explicit protocol stdout
/// instead retains exact bytes under a cumulative allowance, independently of log eviction and
/// consumer polling. Normal protocol chunks and logs alternate; fatal loss and reader failures
/// take priority. Neither policy waits for consumer capacity.
pub(super) struct PlatformOutputQueue {
    record_limit: usize,
    byte_limit: usize,
    state: Mutex<PlatformOutputQueueState>,
    wake: SupervisedPlayWake,
}

impl PlatformOutputQueue {
    #[cfg(test)]
    pub(super) fn new(
        record_limit: usize,
        byte_limit: usize,
        protocol_stdout_limit: Option<NonZeroUsize>,
    ) -> Self {
        Self::with_wake(
            record_limit,
            byte_limit,
            protocol_stdout_limit,
            SupervisedPlayWake::default(),
        )
    }

    pub(super) fn with_wake(
        record_limit: usize,
        byte_limit: usize,
        protocol_stdout_limit: Option<NonZeroUsize>,
        wake: SupervisedPlayWake,
    ) -> Self {
        assert!(
            record_limit > 0,
            "the platform output record limit is non-zero"
        );
        assert!(byte_limit > 0, "the platform output byte limit is non-zero");
        Self {
            record_limit,
            byte_limit,
            state: Mutex::new(PlatformOutputQueueState {
                protocol_stdout: protocol_stdout_limit.map(ProtocolStdoutCapture::new),
                ..PlatformOutputQueueState::default()
            }),
            wake,
        }
    }

    pub(super) fn push(&self, stream: SupervisedProcessStream, bytes: Vec<u8>) -> io::Result<()> {
        self.push_output(PlatformOutput {
            stream,
            bytes,
            loss: false,
        })
    }

    pub(super) fn push_loss(
        &self,
        stream: SupervisedProcessStream,
        bytes: Vec<u8>,
    ) -> io::Result<()> {
        self.push_output(PlatformOutput {
            stream,
            bytes,
            loss: true,
        })
    }

    pub(super) fn pop(&self) -> io::Result<Option<PlatformOutput>> {
        let mut state = self.lock()?;
        // Fatal observations never wait for normal output, even if either stream stays noisy.
        if let Some(loss) = state
            .protocol_stdout
            .as_mut()
            .and_then(|stdout| stdout.loss.take())
        {
            return Ok(Some(loss));
        }
        if let Some(error) = state.failure.take() {
            return Err(error);
        }

        if state.prefer_log_output
            && let Some(output) = state.pop_log_output()
        {
            state.prefer_log_output = false;
            return Ok(Some(output));
        }
        if let Some(output) = state
            .protocol_stdout
            .as_mut()
            .and_then(ProtocolStdoutCapture::pop_chunk)
        {
            state.prefer_log_output = true;
            return Ok(Some(output));
        }
        let output = state.pop_log_output();
        if output.is_some() {
            state.prefer_log_output = false;
        }
        Ok(output)
    }

    pub(super) fn record_failure(&self, error: io::Error) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.failure.is_none() {
                state.failure = Some(error);
            }
        }
        self.wake.notify();
    }

    pub(super) fn notify(&self) {
        self.wake.notify();
    }

    pub(super) fn is_drained(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.is_drained())
    }

    fn push_output(&self, output: PlatformOutput) -> io::Result<()> {
        {
            let mut state = self.lock()?;
            if let (SupervisedProcessStream::Stdout, Some(stdout)) =
                (output.stream, state.protocol_stdout.as_mut())
            {
                stdout.push_output(output, self.byte_limit);
            } else {
                state.push_log_output(output, self.record_limit, self.byte_limit);
            }
        }
        self.wake.notify();
        Ok(())
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, PlatformOutputQueueState>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("the supervised process output queue was poisoned"))
    }
}

#[derive(Default)]
struct PlatformOutputQueueState {
    protocol_stdout: Option<ProtocolStdoutCapture>,
    // Alternate normal protocol chunks and log output whenever both remain pending.
    prefer_log_output: bool,
    records: VecDeque<PlatformOutput>,
    retained_bytes: usize,
    dropped_stdout_bytes: u64,
    dropped_stderr_bytes: u64,
    failure: Option<io::Error>,
}

impl PlatformOutputQueueState {
    fn pop_log_output(&mut self) -> Option<PlatformOutput> {
        if let Some(output) = self.records.pop_front() {
            self.retained_bytes = self.retained_bytes.saturating_sub(output.bytes.len());
            return Some(output);
        }
        if self.dropped_stdout_bytes != 0 {
            let dropped = std::mem::take(&mut self.dropped_stdout_bytes);
            return Some(dropped_output(SupervisedProcessStream::Stdout, dropped));
        }
        if self.dropped_stderr_bytes != 0 {
            let dropped = std::mem::take(&mut self.dropped_stderr_bytes);
            return Some(dropped_output(SupervisedProcessStream::Stderr, dropped));
        }
        None
    }

    fn push_log_output(
        &mut self,
        mut output: PlatformOutput,
        record_limit: usize,
        byte_limit: usize,
    ) {
        if output.bytes.len() > byte_limit {
            let dropped = output.bytes.len() - byte_limit;
            output.bytes.drain(..dropped);
            self.record_drop(output.stream, dropped);
        }
        while self.records.len() >= record_limit
            || self.retained_bytes.saturating_add(output.bytes.len()) > byte_limit
        {
            let Some(evicted) = self.records.pop_front() else {
                break;
            };
            self.retained_bytes = self.retained_bytes.saturating_sub(evicted.bytes.len());
            self.record_drop(evicted.stream, evicted.bytes.len());
        }
        self.retained_bytes = self.retained_bytes.saturating_add(output.bytes.len());
        self.records.push_back(output);
    }

    fn record_drop(&mut self, stream: SupervisedProcessStream, bytes: usize) {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        let target = match stream {
            SupervisedProcessStream::Stdout => &mut self.dropped_stdout_bytes,
            SupervisedProcessStream::Stderr => &mut self.dropped_stderr_bytes,
        };
        *target = target.saturating_add(bytes);
    }

    fn is_drained(&self) -> bool {
        self.protocol_stdout
            .as_ref()
            .is_none_or(ProtocolStdoutCapture::is_drained)
            && self.records.is_empty()
            && self.dropped_stdout_bytes == 0
            && self.dropped_stderr_bytes == 0
            && self.failure.is_none()
    }
}

struct ProtocolStdoutCapture {
    limit: usize,
    // This lifetime total is never decremented when a chunk is consumed.
    admitted_bytes: usize,
    chunks: VecDeque<Vec<u8>>,
    failed: bool,
    loss: Option<PlatformOutput>,
}

impl ProtocolStdoutCapture {
    fn new(limit: NonZeroUsize) -> Self {
        Self {
            limit: limit.get(),
            admitted_bytes: 0,
            chunks: VecDeque::new(),
            failed: false,
            loss: None,
        }
    }

    fn push_output(&mut self, mut output: PlatformOutput, marker_byte_limit: usize) {
        if self.failed {
            return;
        }
        if output.loss {
            output.bytes.truncate(marker_byte_limit);
            self.failed = true;
            self.loss = Some(output);
            return;
        }

        let accepted = output.bytes.len().min(self.limit - self.admitted_bytes);
        let mut bytes = &output.bytes[..accepted];
        while !bytes.is_empty() {
            if self
                .chunks
                .back()
                .is_none_or(|chunk| chunk.len() == PROTOCOL_STDOUT_CHUNK_BYTES)
            {
                self.chunks.push_back(Vec::with_capacity(
                    (self.limit - self.admitted_bytes).min(PROTOCOL_STDOUT_CHUNK_BYTES),
                ));
            }
            let chunk = self
                .chunks
                .back_mut()
                .expect("the protocol chunk was allocated");
            let length = bytes.len().min(PROTOCOL_STDOUT_CHUNK_BYTES - chunk.len());
            chunk.extend_from_slice(&bytes[..length]);
            self.admitted_bytes += length;
            bytes = &bytes[length..];
        }

        if accepted < output.bytes.len() {
            self.failed = true;
            self.loss = Some(PlatformOutput {
                stream: SupervisedProcessStream::Stdout,
                bytes: format!(
                    "[nara-editor: protocol stdout exceeded its cumulative {} byte limit; remaining bytes were not retained]\n",
                    self.limit,
                )
                .into_bytes(),
                loss: true,
            });
        }
    }

    fn pop_chunk(&mut self) -> Option<PlatformOutput> {
        self.chunks.pop_front().map(|bytes| PlatformOutput {
            stream: SupervisedProcessStream::Stdout,
            bytes,
            loss: false,
        })
    }

    fn is_drained(&self) -> bool {
        self.chunks.is_empty() && self.loss.is_none()
    }
}

fn dropped_output(stream: SupervisedProcessStream, dropped: u64) -> PlatformOutput {
    let stream_name = match stream {
        SupervisedProcessStream::Stdout => "stdout",
        SupervisedProcessStream::Stderr => "stderr",
    };
    PlatformOutput {
        stream,
        bytes: format!(
            "[nara-editor: dropped {dropped} {stream_name} bytes before log projection]\n"
        )
        .into_bytes(),
        loss: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_queue_retains_explicit_loss_after_eviction() {
        let queue = PlatformOutputQueue::new(2, 128, None);
        queue
            .push(SupervisedProcessStream::Stdout, b"abcd".to_vec())
            .expect("the first record is accepted");
        queue
            .push(SupervisedProcessStream::Stderr, b"efgh".to_vec())
            .expect("the second record is accepted");
        queue
            .push(SupervisedProcessStream::Stdout, b"ijkl".to_vec())
            .expect("the newest record is retained");

        let mut outputs = Vec::new();
        while let Some(output) = queue.pop().expect("the queue remains readable") {
            outputs.push(output);
        }

        assert!(outputs.iter().any(|output| output.loss));
        assert!(outputs.iter().all(|output| output.bytes.len() <= 128));
        assert!(queue.is_drained());
    }

    #[test]
    fn explicit_loss_marker_survives_the_shared_queue() {
        let queue = PlatformOutputQueue::new(2, 128, None);
        queue
            .push_loss(
                SupervisedProcessStream::Stderr,
                b"capture cancelled\n".to_vec(),
            )
            .expect("the loss marker is accepted");

        let output = queue
            .pop()
            .expect("the queue remains readable")
            .expect("the marker is retained");
        assert_eq!(output.stream, SupervisedProcessStream::Stderr);
        assert_eq!(output.bytes.as_slice(), b"capture cancelled\n");
        assert!(output.loss);
    }

    #[test]
    fn protocol_burst_is_retained_before_the_first_poll() {
        let expected: Vec<_> = (0_u8..=255).cycle().take(1024 * 1024).collect();
        let queue = PlatformOutputQueue::new(2, 128, NonZeroUsize::new(expected.len()));
        for bytes in expected.chunks(8 * 1024) {
            queue
                .push(SupervisedProcessStream::Stdout, bytes.to_vec())
                .unwrap();
        }
        assert!(!queue.is_drained());

        let mut captured = Vec::new();
        while let Some(output) = queue.pop().unwrap() {
            assert_eq!(output.stream, SupervisedProcessStream::Stdout);
            assert!(!output.loss);
            assert!(output.bytes.len() <= PROTOCOL_STDOUT_CHUNK_BYTES);
            captured.extend(output.bytes);
        }
        assert_eq!(captured, expected);
        assert!(queue.is_drained());
    }

    #[test]
    fn normal_protocol_chunks_and_pending_stderr_share_the_first_poll_budget() {
        let expected: Vec<_> = (0_u8..=255).cycle().take(2 * 1024 * 1024).collect();
        let queue = PlatformOutputQueue::new(128, 256 * 1024, NonZeroUsize::new(expected.len()));
        queue
            .push(SupervisedProcessStream::Stdout, expected.clone())
            .unwrap();
        for index in 0..64_u8 {
            queue
                .push(SupervisedProcessStream::Stderr, vec![index])
                .unwrap();
        }

        let mut captured = Vec::new();
        let mut stderr = Vec::new();
        for _ in 0..32 {
            let stdout = queue.pop().unwrap().unwrap();
            assert_eq!(stdout.stream, SupervisedProcessStream::Stdout);
            assert!(!stdout.loss);
            assert_eq!(stdout.bytes.len(), PROTOCOL_STDOUT_CHUNK_BYTES);
            captured.extend(stdout.bytes);

            let diagnostic = queue.pop().unwrap().unwrap();
            assert_eq!(diagnostic.stream, SupervisedProcessStream::Stderr);
            assert!(!diagnostic.loss);
            stderr.extend(diagnostic.bytes);
        }
        assert_eq!(stderr, (0..32_u8).collect::<Vec<_>>());
        assert!(
            !queue.is_drained(),
            "the first 64 outputs do not exhaust either stream"
        );
        while let Some(output) = queue.pop().unwrap() {
            assert!(!output.loss);
            match output.stream {
                SupervisedProcessStream::Stdout => captured.extend(output.bytes),
                SupervisedProcessStream::Stderr => stderr.extend(output.bytes),
            }
        }
        assert_eq!(captured, expected);
        assert_eq!(stderr, (0..64_u8).collect::<Vec<_>>());
        assert!(queue.is_drained());
    }

    #[test]
    fn tiny_protocol_reads_coalesce_and_survive_stderr_eviction() {
        let expected: Vec<_> = (0_u8..=255).cycle().take(64 * 1024 + 7).collect();
        let queue = PlatformOutputQueue::new(1, 32, NonZeroUsize::new(expected.len()));
        for (index, byte) in expected.iter().copied().enumerate() {
            queue
                .push(SupervisedProcessStream::Stdout, vec![byte])
                .unwrap();
            if index % 31 == 0 {
                queue
                    .push(SupervisedProcessStream::Stderr, vec![b'e'; 32])
                    .unwrap();
            }
        }

        let mut captured = Vec::new();
        let mut stdout_records = 0;
        let mut stderr_loss = false;
        while let Some(output) = queue.pop().unwrap() {
            match output.stream {
                SupervisedProcessStream::Stdout => {
                    assert!(!output.loss);
                    stdout_records += 1;
                    captured.extend(output.bytes);
                }
                SupervisedProcessStream::Stderr => stderr_loss |= output.loss,
            }
        }
        assert_eq!(captured, expected);
        assert!(
            stdout_records <= 8,
            "small OS reads do not become individual records"
        );
        assert!(stderr_loss);
        assert!(queue.is_drained());
    }

    #[test]
    fn protocol_allowance_remains_cumulative_after_consumption() {
        let queue = PlatformOutputQueue::new(2, 128, NonZeroUsize::new(5));
        for bytes in [b"abc".as_slice(), b"de".as_slice()] {
            queue
                .push(SupervisedProcessStream::Stdout, bytes.to_vec())
                .unwrap();
            let output = queue.pop().unwrap().unwrap();
            assert_eq!(output.bytes, bytes);
            assert!(!output.loss);
            assert!(queue.is_drained());
        }

        queue
            .push(SupervisedProcessStream::Stdout, b"f".to_vec())
            .unwrap();
        assert!(!queue.is_drained());
        let overflow = queue.pop().unwrap().unwrap();
        assert_eq!(overflow.stream, SupervisedProcessStream::Stdout);
        assert!(overflow.loss);
        assert!(String::from_utf8_lossy(&overflow.bytes).contains("cumulative 5 byte limit"));
        assert!(queue.is_drained());

        queue
            .push(SupervisedProcessStream::Stdout, b"later bytes".to_vec())
            .unwrap();
        assert!(queue.pop().unwrap().is_none());
        assert!(queue.is_drained());
    }

    #[test]
    fn protocol_one_byte_overflow_keeps_the_prefix_and_a_reliable_failure() {
        let queue = PlatformOutputQueue::new(1, 128, NonZeroUsize::new(4));
        queue
            .push(SupervisedProcessStream::Stdout, b"abcde".to_vec())
            .unwrap();
        queue
            .push(SupervisedProcessStream::Stdout, b"ignored".to_vec())
            .unwrap();
        for _ in 0..8 {
            queue
                .push(SupervisedProcessStream::Stderr, vec![b'e'; 128])
                .unwrap();
        }

        let overflow = queue.pop().unwrap().unwrap();
        assert_eq!(overflow.stream, SupervisedProcessStream::Stdout);
        assert!(overflow.loss);
        assert!(!queue.is_drained());
        let prefix = queue.pop().unwrap().unwrap();
        assert_eq!(prefix.stream, SupervisedProcessStream::Stdout);
        assert_eq!(prefix.bytes, b"abcd");
        assert!(!prefix.loss);
        assert!(!queue.is_drained());
        let mut stderr_loss = false;
        while let Some(output) = queue.pop().unwrap() {
            assert_eq!(output.stream, SupervisedProcessStream::Stderr);
            stderr_loss |= output.loss;
        }
        assert!(stderr_loss);
        assert!(queue.is_drained());
    }

    #[test]
    fn fatal_protocol_loss_precedes_reader_failure_and_normal_output() {
        let queue = PlatformOutputQueue::new(1, 128, NonZeroUsize::new(2));
        queue
            .push(SupervisedProcessStream::Stdout, b"a".to_vec())
            .unwrap();
        assert_eq!(queue.pop().unwrap().unwrap().bytes, b"a");
        // A normal stdout pop makes logs next in turn, but fatal observations still come first.
        queue
            .push(SupervisedProcessStream::Stdout, b"bc".to_vec())
            .unwrap();
        queue
            .push(SupervisedProcessStream::Stderr, b"diagnostic".to_vec())
            .unwrap();
        queue.record_failure(io::Error::new(io::ErrorKind::BrokenPipe, "reader failed"));

        let loss = queue.pop().unwrap().unwrap();
        assert_eq!(loss.stream, SupervisedProcessStream::Stdout);
        assert!(loss.loss);
        let error = queue
            .pop()
            .err()
            .expect("the reader failure precedes normal output");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(!queue.is_drained());
        let diagnostic = queue.pop().unwrap().unwrap();
        assert_eq!(diagnostic.stream, SupervisedProcessStream::Stderr);
        assert_eq!(diagnostic.bytes, b"diagnostic");
        let prefix = queue.pop().unwrap().unwrap();
        assert_eq!(prefix.stream, SupervisedProcessStream::Stdout);
        assert_eq!(prefix.bytes, b"b");
        assert!(!prefix.loss);
        assert!(queue.is_drained());
    }

    #[test]
    fn cancelled_protocol_capture_keeps_its_prefix_and_closes_unused_allowance() {
        let queue = PlatformOutputQueue::new(1, 128, NonZeroUsize::new(4));
        queue
            .push(SupervisedProcessStream::Stdout, b"ab".to_vec())
            .unwrap();
        queue
            .push_loss(
                SupervisedProcessStream::Stdout,
                b"capture cancelled\n".to_vec(),
            )
            .unwrap();
        queue
            .push(SupervisedProcessStream::Stdout, b"cd".to_vec())
            .unwrap();
        queue
            .push_loss(SupervisedProcessStream::Stdout, b"later loss\n".to_vec())
            .unwrap();
        for _ in 0..4 {
            queue
                .push(SupervisedProcessStream::Stderr, vec![b'e'; 128])
                .unwrap();
        }

        let marker = queue.pop().unwrap().unwrap();
        assert_eq!(marker.stream, SupervisedProcessStream::Stdout);
        assert_eq!(marker.bytes, b"capture cancelled\n");
        assert!(marker.loss);
        let prefix = queue.pop().unwrap().unwrap();
        assert_eq!(prefix.stream, SupervisedProcessStream::Stdout);
        assert_eq!(prefix.bytes, b"ab");
        assert!(!prefix.loss);
        while let Some(output) = queue.pop().unwrap() {
            assert_eq!(output.stream, SupervisedProcessStream::Stderr);
        }
        assert!(queue.is_drained());
    }

    #[test]
    fn reader_failure_and_pending_protocol_bytes_both_require_observation() {
        for protocol_limit in [None, NonZeroUsize::new(2)] {
            let queue = PlatformOutputQueue::new(1, 128, protocol_limit);
            if protocol_limit.is_some() {
                queue
                    .push(SupervisedProcessStream::Stdout, b"ab".to_vec())
                    .unwrap();
            }
            queue.record_failure(io::Error::new(io::ErrorKind::BrokenPipe, "reader failed"));
            queue.record_failure(io::Error::other("later failure"));
            assert!(!queue.is_drained());

            let mut original_error = None;
            let mut captured = Vec::new();
            for attempt in 0..4 {
                // Refill stderr before every pop; a failure behind normal output would starve.
                queue
                    .push(SupervisedProcessStream::Stderr, b"diagnostic".to_vec())
                    .unwrap();
                match queue.pop() {
                    Err(error) => {
                        assert_eq!(attempt, 0, "the reader failure precedes normal data");
                        original_error = Some(error);
                        break;
                    }
                    Ok(Some(output)) => {
                        assert!(!output.loss);
                        if output.stream == SupervisedProcessStream::Stdout {
                            captured.extend(output.bytes);
                        }
                    }
                    Ok(None) => panic!("the reader failure must remain pending"),
                }
            }
            let error = original_error.expect("the original reader failure is delivered promptly");
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(error.to_string(), "reader failed");
            assert!(
                !queue.is_drained(),
                "consuming the error does not discard pending data"
            );

            while let Some(output) = queue.pop().unwrap() {
                if output.stream == SupervisedProcessStream::Stdout && !output.loss {
                    captured.extend(output.bytes);
                }
            }
            if protocol_limit.is_some() {
                assert_eq!(captured, b"ab");
            }
            assert!(queue.is_drained());
            assert!(queue.pop().unwrap().is_none());
        }
    }
}
