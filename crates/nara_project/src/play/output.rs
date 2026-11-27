use std::{
    io::{self, PipeReader, PipeWriter},
    process::ExitStatus,
    time::Instant,
};

use super::{DRAIN_TIMEOUT, OutputLine, OutputStream, native};

const READ_BUDGET: usize = 64 * 1024;
const READ_CHUNK: usize = 8 * 1024;
const MAX_LINE_BYTES: usize = 256 * 1024;
const MAX_PENDING_BYTES: usize = MAX_LINE_BYTES + b"\r\n".len();

pub(super) struct LinePipe {
    reader: Option<PipeReader>,
    stream: OutputStream,
    pending: Vec<u8>,
}

impl LinePipe {
    pub(super) fn new(stream: OutputStream) -> io::Result<(Self, PipeWriter)> {
        let (reader, writer) = io::pipe()?;
        #[cfg(unix)]
        native::set_nonblocking(&reader)?;
        Ok((
            Self {
                reader: Some(reader),
                stream,
                pending: Vec::new(),
            },
            writer,
        ))
    }

    pub(super) fn drain(
        &mut self,
        exit: Option<(ExitStatus, Instant)>,
        emit: &mut impl FnMut(OutputLine),
    ) -> io::Result<()> {
        let stream = self.stream;
        let pending = &mut self.pending;
        let result = drain_pipe(&mut self.reader, exit, |chunk| {
            for part in chunk.split_inclusive(|byte| *byte == b'\n') {
                let keep = part.len().min(MAX_PENDING_BYTES - pending.len());
                pending.extend_from_slice(&part[..keep]);
                if part.ends_with(b"\n") {
                    emit_line(stream, pending, emit);
                }
            }
            Ok(())
        });
        if self.reader.is_none() && !pending.is_empty() {
            emit_line(stream, pending, emit);
        }
        result.map(|_| ())
    }

    pub(super) fn drain_raw(
        &mut self,
        exit: Option<(ExitStatus, Instant)>,
        consume: impl FnMut(&[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        match drain_pipe(&mut self.reader, exit, consume)? {
            Drain::Pending | Drain::Eof => Ok(()),
            Drain::TimedOut => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "producer stdout did not close before the drain deadline",
            )),
        }
    }

    pub(super) fn is_closed(&self) -> bool {
        self.reader.is_none()
    }
}

enum Drain {
    Pending,
    Eof,
    TimedOut,
}

fn drain_pipe(
    reader: &mut Option<PipeReader>,
    exit: Option<(ExitStatus, Instant)>,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<Drain> {
    let Some(pipe) = reader.as_mut() else {
        return Ok(Drain::Eof);
    };
    let mut remaining = READ_BUDGET;
    let mut buffer = [0_u8; READ_CHUNK];
    let result = loop {
        if remaining == 0 {
            break Ok(pending_or_timed_out(exit));
        }
        let limit = remaining.min(buffer.len());
        match native::read_pipe(pipe, &mut buffer[..limit]) {
            Ok(0) => break Ok(Drain::Eof),
            Ok(count) => {
                remaining -= count;
                if let Err(error) = consume(&buffer[..count]) {
                    break Err(error);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                break Ok(pending_or_timed_out(exit));
            }
            Err(error) => break Err(error),
        }
    };
    if !matches!(result, Ok(Drain::Pending)) {
        *reader = None;
    }
    result
}

fn pending_or_timed_out(exit: Option<(ExitStatus, Instant)>) -> Drain {
    if exit.is_some_and(|(_, time)| time.elapsed() >= DRAIN_TIMEOUT) {
        Drain::TimedOut
    } else {
        Drain::Pending
    }
}

fn emit_line(stream: OutputStream, pending: &mut Vec<u8>, emit: &mut impl FnMut(OutputLine)) {
    let mut bytes = pending.as_slice();
    if let Some(line) = bytes.strip_suffix(b"\n") {
        bytes = line.strip_suffix(b"\r").unwrap_or(line);
    }
    let mut text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_LINE_BYTES)]).into_owned();
    if bytes.len() > MAX_LINE_BYTES {
        text.push_str(" [truncated]");
    }
    pending.clear();
    emit(OutputLine { stream, text });
}

#[cfg(test)]
mod tests {
    use std::{io::Write as _, thread, time::Duration};

    use super::*;

    #[test]
    fn raw_stdout_keeps_line_endings_and_invalid_utf8() {
        let (mut pipe, mut writer) = LinePipe::new(OutputStream::Stdout).unwrap();
        let input = b"\xff\r\ntail";
        writer.write_all(input).unwrap();
        drop(writer);

        let mut bytes = Vec::new();
        pipe.drain_raw(None, |chunk| {
            bytes.extend_from_slice(chunk);
            Ok(())
        })
        .unwrap();
        assert_eq!(bytes, input);
        assert!(pipe.is_closed());
    }

    #[test]
    fn long_lines_are_bounded_and_resume_at_the_next_newline() {
        let (mut pipe, mut writer) = LinePipe::new(OutputStream::Stdout).unwrap();
        let producer = thread::spawn(move || {
            let block = vec![b'x'; MAX_LINE_BYTES];
            writer.write_all(&block).unwrap();
            writer.write_all(&block).unwrap();
            writer.write_all(b"NARA_EDITOR ready\n").unwrap();
            writer.write_all(&block).unwrap();
            writer.write_all(b"\r\nnext\r\n").unwrap();
            writer.write_all(&block).unwrap();
            writer.write_all(b"tail").unwrap();
        });

        let mut lines = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pipe.is_closed() {
            assert!(Instant::now() < deadline, "producer did not close its pipe");
            pipe.drain(None, &mut |line| lines.push(line.text)).unwrap();
            assert!(pipe.pending.len() <= MAX_PENDING_BYTES);
            thread::sleep(Duration::from_millis(1));
        }
        producer.join().unwrap();

        assert_eq!(lines.len(), 4);
        let prefix = "x".repeat(MAX_LINE_BYTES);
        for index in [0, 3] {
            assert_eq!(lines[index].strip_prefix(&prefix), Some(" [truncated]"));
        }
        assert!(lines[1] == prefix);
        assert_eq!(lines[2], "next");
    }
}
