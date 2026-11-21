use std::{
    io::{self, Write},
    sync::mpsc::{self, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::Value;

const QUEUE_CAPACITY: usize = 16;
const FINISH_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct Output {
    sender: SyncSender<(&'static str, Value)>,
    writer: JoinHandle<io::Result<()>>,
}

impl Output {
    pub(super) fn spawn() -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let writer = thread::Builder::new()
            .name("two-level-server-output".into())
            .spawn(move || {
                let stdout = io::stdout();
                for (prefix, record) in receiver {
                    let mut stdout = stdout.lock();
                    writeln!(stdout, "{prefix} {record}")?;
                    stdout.flush()?;
                }
                Ok(())
            })?;
        Ok(Self { sender, writer })
    }

    pub(super) fn send(&self, prefix: &'static str, record: Value) -> io::Result<()> {
        self.sender
            .try_send((prefix, record))
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    io::Error::new(io::ErrorKind::WouldBlock, "Server stdout queue is full")
                }
                TrySendError::Disconnected(_) => writer_disconnected(),
            })
    }

    pub(super) fn check(&self) -> io::Result<()> {
        if self.writer.is_finished() {
            return Err(writer_disconnected());
        }
        Ok(())
    }

    pub(super) fn finish(self, prefix: &'static str, record: Value) -> io::Result<()> {
        let deadline = Instant::now() + FINISH_TIMEOUT;
        let Self { sender, writer } = self;
        let wait_to_poll = || {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out waiting for Server stdout writer to finish",
                ));
            }
            thread::sleep(remaining.min(Duration::from_millis(5)));
            Ok(())
        };

        let mut record = (prefix, record);
        let queued = loop {
            match sender.try_send(record) {
                Ok(()) => break Ok(()),
                Err(TrySendError::Full(pending)) => {
                    record = pending;
                    wait_to_poll()?;
                }
                Err(TrySendError::Disconnected(_)) => break Err(writer_disconnected()),
            }
        };
        drop(sender);

        while !writer.is_finished() {
            wait_to_poll()?;
        }

        writer
            .join()
            .map_err(|_| io::Error::other("Server stdout writer panicked"))??;
        queued
    }
}

fn writer_disconnected() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "Server stdout writer disconnected",
    )
}
