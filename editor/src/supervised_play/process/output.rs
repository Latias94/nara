use std::{
    collections::VecDeque,
    io,
    sync::{Mutex, MutexGuard},
};

use super::{PlatformOutput, SupervisedProcessStream};
use crate::supervised_play::wake::SupervisedPlayWake;

/// Private, thread-safe queue between platform readers and the supervised-process facade.
///
/// The queue retains the newest bounded output and turns every discarded byte into a later loss
/// marker. Platform adapters therefore share one overflow contract without exposing a generic
/// process or logging abstraction outside supervised Play.
pub(super) struct PlatformOutputQueue {
    record_limit: usize,
    byte_limit: usize,
    state: Mutex<PlatformOutputQueueState>,
    wake: SupervisedPlayWake,
}

impl PlatformOutputQueue {
    pub(super) fn new(record_limit: usize, byte_limit: usize) -> Self {
        Self::with_wake(record_limit, byte_limit, SupervisedPlayWake::default())
    }

    pub(super) fn with_wake(
        record_limit: usize,
        byte_limit: usize,
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
            state: Mutex::new(PlatformOutputQueueState::default()),
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
        if let Some(output) = state.records.pop_front() {
            state.retained_bytes = state.retained_bytes.saturating_sub(output.bytes.len());
            return Ok(Some(output));
        }
        if state.dropped_stdout_bytes != 0 {
            let dropped = std::mem::take(&mut state.dropped_stdout_bytes);
            return Ok(Some(dropped_output(
                SupervisedProcessStream::Stdout,
                dropped,
            )));
        }
        if state.dropped_stderr_bytes != 0 {
            let dropped = std::mem::take(&mut state.dropped_stderr_bytes);
            return Ok(Some(dropped_output(
                SupervisedProcessStream::Stderr,
                dropped,
            )));
        }
        if let Some(error) = state.failure.take() {
            return Err(error);
        }
        Ok(None)
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

    fn push_output(&self, mut output: PlatformOutput) -> io::Result<()> {
        {
            let mut state = self.lock()?;
            if output.bytes.len() > self.byte_limit {
                let dropped = output.bytes.len() - self.byte_limit;
                output.bytes.drain(..dropped);
                state.record_drop(output.stream, dropped);
            }
            while state.records.len() >= self.record_limit
                || state.retained_bytes.saturating_add(output.bytes.len()) > self.byte_limit
            {
                let Some(evicted) = state.records.pop_front() else {
                    break;
                };
                state.retained_bytes = state.retained_bytes.saturating_sub(evicted.bytes.len());
                state.record_drop(evicted.stream, evicted.bytes.len());
            }
            state.retained_bytes = state.retained_bytes.saturating_add(output.bytes.len());
            state.records.push_back(output);
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
    records: VecDeque<PlatformOutput>,
    retained_bytes: usize,
    dropped_stdout_bytes: u64,
    dropped_stderr_bytes: u64,
    failure: Option<io::Error>,
}

impl PlatformOutputQueueState {
    fn record_drop(&mut self, stream: SupervisedProcessStream, bytes: usize) {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        let target = match stream {
            SupervisedProcessStream::Stdout => &mut self.dropped_stdout_bytes,
            SupervisedProcessStream::Stderr => &mut self.dropped_stderr_bytes,
        };
        *target = target.saturating_add(bytes);
    }

    fn is_drained(&self) -> bool {
        self.records.is_empty()
            && self.dropped_stdout_bytes == 0
            && self.dropped_stderr_bytes == 0
            && self.failure.is_none()
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
        let queue = PlatformOutputQueue::new(2, 128);
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
        let queue = PlatformOutputQueue::new(2, 128);
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
}
